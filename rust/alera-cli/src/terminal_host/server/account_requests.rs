use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::terminal_host::alera_account::{wait_for_callback, AuthProvider};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, event, ok_response};

use super::account_push_state::{
    account_error, async_account_delete, async_account_sign_out, async_account_transfer,
    async_mobile_enrollment, prepare_sign_in,
};
use super::deferred_admission::DeferredRequestClass;
use super::request_payloads::parse_payload;
use super::requests::require_string_key;
use super::{ServerActor, ServerCommand};

#[derive(Debug, Clone, Copy)]
pub(crate) enum AccountOperation {
    Configuration,
    SignOut,
    Delete,
    Transfer,
    MobileEnrollment,
}

impl AccountOperation {
    fn request_type(self) -> &'static str {
        match self {
            Self::Configuration => "configuration.cloud",
            Self::SignOut => "account.signOut",
            Self::Delete => "account.delete",
            Self::Transfer => "account.transfer.confirm",
            Self::MobileEnrollment => "mobile.cloudEnrollment.create",
        }
    }
}

pub(crate) enum AccountCommand {
    SignInPrepared {
        client_id: u64,
        request_id: i64,
        result: HostResult<Value>,
    },
    SignInCompleted {
        result: HostResult<Value>,
    },
    OperationFinished {
        client_id: u64,
        request_id: i64,
        operation: AccountOperation,
        result: HostResult<Value>,
    },
    SubscriptionSyncFinished {
        result: HostResult<usize>,
    },
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignInRequest {
    provider: AuthProvider,
}

impl ServerActor {
    pub(super) async fn handle_account_command(&mut self, command: AccountCommand) {
        match command {
            AccountCommand::SignInPrepared {
                client_id,
                request_id,
                result,
            } => self.handle_account_sign_in_prepared(client_id, request_id, result),
            AccountCommand::SignInCompleted { result } => {
                self.handle_account_sign_in_completed(result).await
            }
            AccountCommand::OperationFinished {
                client_id,
                request_id,
                operation,
                result,
            } => {
                self.handle_account_operation_finished(client_id, request_id, operation, result)
                    .await
            }
            AccountCommand::SubscriptionSyncFinished { result } => {
                self.handle_push_subscription_sync_finished(result)
            }
        }
    }

    pub(super) fn try_start_account_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<bool> {
        if (request_type.starts_with("account.") && request_type != "account.status")
            || request_type.starts_with("mobile.cloud")
        {
            return Err(HostError::state(
                "Alera Cloud is disabled in this privacy portable build.",
            ));
        }
        match request_type {
            "account.signIn.start" | "account.link.start" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let request: SignInRequest = parse_payload(payload)?;
                self.start_account_sign_in(
                    client_id,
                    request_id,
                    request.provider,
                    request_type == "account.link.start",
                )?;
                Ok(true)
            }
            "account.signOut" => {
                self.require_local_account_request(client_id, request_type)?;
                self.start_account_operation(
                    client_id,
                    request_id,
                    AccountOperation::SignOut,
                    async_account_sign_out(self.account_push.service.clone()),
                );
                Ok(true)
            }
            "account.delete" => {
                self.require_local_account_request(client_id, request_type)?;
                self.start_account_operation(
                    client_id,
                    request_id,
                    AccountOperation::Delete,
                    async_account_delete(self.account_push.service.clone()),
                );
                Ok(true)
            }
            "account.transfer.confirm" => {
                self.require_local_account_request(client_id, request_type)?;
                let target = require_string_key(payload, "targetAccountId")?;
                self.start_account_operation(
                    client_id,
                    request_id,
                    AccountOperation::Transfer,
                    async_account_transfer(self.account_push.service.clone(), target),
                );
                Ok(true)
            }
            "mobile.cloudEnrollment.create" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let client = self
                    .clients
                    .get(&client_id)
                    .ok_or_else(|| HostError::state("Client disconnected."))?;
                let device_id = client.cloud_device_id.clone().ok_or_else(|| {
                    HostError::state(
                        "mobile.hello must include cloudDeviceId before cloud enrollment.",
                    )
                })?;
                let device_name = client
                    .mobile_device_name
                    .clone()
                    .unwrap_or_else(|| "Alera Mobile".to_string());
                self.start_account_operation(
                    client_id,
                    request_id,
                    AccountOperation::MobileEnrollment,
                    async_mobile_enrollment(
                        self.account_push.service.clone(),
                        device_id,
                        device_name,
                    ),
                );
                Ok(true)
            }
            "mobile.cloudSubscriptions.refresh" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_push_subscription_sync(Some((client_id, request_id)));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub(super) async fn account_status(&self) -> HostResult<Value> {
        let account = self
            .account_push
            .service
            .local_account()
            .await
            .map_err(account_error)?;
        Ok(json!({
            "connected": account.is_some(),
            "account": account,
            "signInPending": self.account_push.sign_in_cancel.is_some(),
        }))
    }

    pub(super) fn cancel_account_sign_in(&mut self) -> Value {
        let cancelled = self
            .account_push
            .sign_in_cancel
            .take()
            .is_some_and(|cancel| cancel.send(()).is_ok());
        json!({ "cancelled": cancelled })
    }

    fn require_local_account_request(&self, client_id: u64, request_type: &str) -> HostResult<()> {
        self.require_auth(client_id)?;
        self.require_request_allowed(client_id, request_type)
    }

    pub(super) fn start_account_sign_in(
        &mut self,
        client_id: u64,
        request_id: i64,
        provider: AuthProvider,
        linking: bool,
    ) -> HostResult<()> {
        if self.account_push.sign_in_cancel.is_some() {
            return Err(HostError::state(
                "An Alera account sign-in is already in progress.",
            ));
        }
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let inbox = self.inbox.clone();
        let service = self.account_push.service.clone();
        let request_type = if linking {
            "account.link.start"
        } else {
            "account.signIn.start"
        };
        self.admit_cloud_job(request_type, None, async move {
            let preparation = prepare_sign_in(&service, provider, linking).await;
            let (listener, redirect_uri, pkce, transaction) = match preparation {
                Ok(value) => value,
                Err(error) => {
                    let _ = inbox.send(ServerCommand::Account(AccountCommand::SignInPrepared {
                        client_id,
                        request_id,
                        result: Err(account_error(error)),
                    }));
                    return;
                }
            };
            let _ = inbox.send(ServerCommand::Account(AccountCommand::SignInPrepared {
                client_id,
                request_id,
                result: Ok(json!({
                    "authorizationUrl": transaction.authorization_url,
                    "expiresAt": transaction.expires_at,
                    "provider": provider,
                })),
            }));
            let result = async {
                let code = wait_for_callback(listener, &transaction.state, cancel_rx).await?;
                service
                    .exchange_auth(
                        &transaction.transaction_id,
                        &transaction.state,
                        &code,
                        &pkce.verifier,
                    )
                    .await
            }
            .await
            .map(|account| json!(account))
            .map_err(account_error);
            let _ = redirect_uri;
            let _ = inbox.send(ServerCommand::Account(AccountCommand::SignInCompleted {
                result,
            }));
        })?;
        self.account_push.sign_in_cancel = Some(cancel_tx);
        Ok(())
    }

    pub(super) fn start_account_operation<F>(
        &mut self,
        client_id: u64,
        request_id: i64,
        operation: AccountOperation,
        future: F,
    ) where
        F: std::future::Future<Output = HostResult<Value>> + Send + 'static,
    {
        let inbox = self.inbox.clone();
        let task = async move {
            let result = future.await;
            let _ = inbox.send(ServerCommand::Account(AccountCommand::OperationFinished {
                client_id,
                request_id,
                operation,
                result,
            }));
        };
        if let Err(error) = self.admit_cloud_job(operation.request_type(), Some(client_id), task) {
            self.client_write(client_id, error_response(request_id, &error));
        }
    }

    pub(super) fn handle_account_sign_in_prepared(
        &mut self,
        client_id: u64,
        request_id: i64,
        result: HostResult<Value>,
    ) {
        match result {
            Ok(payload) => self.client_write(client_id, ok_response(request_id, payload)),
            Err(error) => {
                self.client_write(client_id, error_response(request_id, &error));
                self.account_push.sign_in_cancel = None;
                self.account_push.cloud_jobs = self.account_push.cloud_jobs.saturating_sub(1);
                self.schedule_shutdown_if_idle();
            }
        }
    }

    pub(super) async fn handle_account_sign_in_completed(&mut self, result: HostResult<Value>) {
        self.account_push.sign_in_cancel = None;
        self.account_push.cloud_jobs = self.account_push.cloud_jobs.saturating_sub(1);
        match result {
            Ok(account) => {
                self.restart_remote_relay().await;
                self.broadcast_authenticated(event(
                    "aleraAccountChanged",
                    json!({ "connected": true, "account": account }),
                ));
                if self.account_push.push_enabled {
                    self.start_push_subscription_sync(None);
                }
            }
            Err(error) => self.broadcast_authenticated(event(
                "aleraAccountSignInFailed",
                json!({ "message": error.wire_message() }),
            )),
        }
        self.schedule_shutdown_if_idle();
    }

    pub(super) fn start_push_subscription_sync(&mut self, waiter: Option<(u64, i64)>) {
        if let Some(waiter) = waiter {
            self.account_push.subscription_sync_waiters.push(waiter);
        }
        if self.account_push.subscription_sync_in_flight {
            return;
        }
        self.account_push.subscription_sync_in_flight = true;
        let inbox = self.inbox.clone();
        let service = self.account_push.service.clone();
        let task = async move {
            let result = service
                .refresh_push_subscriptions()
                .await
                .map_err(account_error);
            let _ = inbox.send(ServerCommand::Account(
                AccountCommand::SubscriptionSyncFinished { result },
            ));
        };
        if let Err(error) = self.admit_cloud_job("mobile.cloudSubscriptions.refresh", None, task) {
            self.account_push.subscription_sync_in_flight = false;
            let waiters = std::mem::take(&mut self.account_push.subscription_sync_waiters);
            for (client_id, request_id) in waiters {
                self.client_write(client_id, error_response(request_id, &error));
            }
        }
    }

    pub(super) fn handle_push_subscription_sync_finished(&mut self, result: HostResult<usize>) {
        self.account_push.subscription_sync_in_flight = false;
        self.account_push.cloud_jobs = self.account_push.cloud_jobs.saturating_sub(1);
        let waiters = std::mem::take(&mut self.account_push.subscription_sync_waiters);
        match result {
            Ok(active_subscriptions) => {
                self.account_push.active_subscriptions = if self.account_push.push_enabled {
                    active_subscriptions
                } else {
                    0
                };
                for (client_id, request_id) in waiters {
                    self.client_write(
                        client_id,
                        ok_response(
                            request_id,
                            json!({ "activeSubscriptions": active_subscriptions }),
                        ),
                    );
                }
                self.broadcast_authenticated(event(
                    "mobilePushSubscriptionsChanged",
                    json!({ "activeSubscriptions": active_subscriptions }),
                ));
            }
            Err(error) => {
                if waiters.is_empty() {
                    eprintln!(
                        "alera push subscription sync failed: {}",
                        error.wire_message()
                    );
                } else {
                    for (client_id, request_id) in waiters {
                        self.client_write(client_id, error_response(request_id, &error));
                    }
                }
            }
        }
        self.schedule_shutdown_if_idle();
    }

    pub(super) async fn handle_account_operation_finished(
        &mut self,
        client_id: u64,
        request_id: i64,
        operation: AccountOperation,
        result: HostResult<Value>,
    ) {
        self.account_push.cloud_jobs = self.account_push.cloud_jobs.saturating_sub(1);
        match result {
            Ok(payload) => {
                self.client_write(client_id, ok_response(request_id, payload));
                if matches!(
                    operation,
                    AccountOperation::SignOut
                        | AccountOperation::Delete
                        | AccountOperation::Transfer
                ) {
                    self.stop_remote_relay().await;
                    self.account_push.active_subscriptions = 0;
                    self.broadcast_authenticated(event(
                        "aleraAccountChanged",
                        json!({ "connected": false }),
                    ));
                }
            }
            Err(error) => self.client_write(client_id, error_response(request_id, &error)),
        }
        self.schedule_shutdown_if_idle();
    }

    fn admit_cloud_job<F>(
        &mut self,
        request_type: &str,
        client_id: Option<u64>,
        task: F,
    ) -> HostResult<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.deferred_admission.schedule(
            DeferredRequestClass::Bulk,
            request_type,
            client_id,
            task,
        )?;
        self.account_push.cloud_jobs += 1;
        self.cancel_shutdown_timer();
        Ok(())
    }
}
