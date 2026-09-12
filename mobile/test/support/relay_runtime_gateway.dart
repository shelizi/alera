import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/accounts/application/cloud_relay_identity_repository.dart';
import 'package:alera_mobile/src/features/accounts/domain/cloud_account_session.dart';
import 'package:alera_mobile/src/features/accounts/infra/alera_cloud_api.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:alera_mobile/src/features/runtime/infra/relay_crypto.dart';
import 'package:alera_mobile/src/features/runtime/infra/relay_wire.dart';

final class RelayRuntimeGateway {
  RelayRuntimeGateway._(
    this._server,
    this._subscription,
    this.runtimeIdentity,
    this.runtimeId,
    this.supportsRenewal,
  );

  static Future<RelayRuntimeGateway> start({
    String runtimeId = 'runtime-1',
    bool supportsRenewal = false,
  }) async {
    final identity = await RelayIdentityKeyPair.generate();
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    late final RelayRuntimeGateway gateway;
    final subscription = server.listen((request) async {
      final socket = await WebSocketTransformer.upgrade(
        request,
        protocolSelector: (_) => relayControlProtocol,
      );
      gateway._sockets.add(socket);
      gateway._served.add(gateway._serve(socket));
    });
    gateway = RelayRuntimeGateway._(
      server,
      subscription,
      identity,
      runtimeId,
      supportsRenewal,
    );
    return gateway;
  }

  final HttpServer _server;
  final StreamSubscription<HttpRequest> _subscription;
  final List<WebSocket> _sockets = <WebSocket>[];
  final List<Future<void>> _served = <Future<void>>[];

  final RelayIdentityKeyPair runtimeIdentity;
  final String runtimeId;
  final bool supportsRenewal;
  final List<Map<String, Object?>> hellos = <Map<String, Object?>>[];
  final List<String> renewalOrder = <String>[];
  final Completer<void> runtimeRenewalStarted = Completer<void>();
  final Completer<void> edgeRenewalStarted = Completer<void>();

  Uri get relayUrl => Uri.parse('ws://127.0.0.1:${_server.port}');

  String get runtimePublicKey =>
      base64UrlNoPadding(runtimeIdentity.publicBytes);

  List<WebSocket> get sockets => List<WebSocket>.unmodifiable(_sockets);

  Future<void> closeSocket(int index) => _sockets[index].close();

  Future<void> _serve(WebSocket socket) async {
    RelayCryptoSession? session;
    var confirmed = false;
    final fragments = RelayFragmentReassembler();
    try {
      await for (final raw in socket) {
        final wire = raw as List<int>;
        if (wire.length >= 2 && wire[0] == 0 && wire[1] == 0) {
          final request =
              jsonDecode(utf8.decode(wire.sublist(2))) as Map<String, Object?>;
          renewalOrder.add('edge');
          if (!edgeRenewalStarted.isCompleted) {
            edgeRenewalStarted.complete();
          }
          final token = request['grant'] as String;
          final claims = _grantClaims(token);
          _replyBinary(socket, <int>[
            0,
            0,
            ...utf8.encode(
              jsonEncode(<String, Object?>{
                'type': 'auth.renewed',
                'id': request['id'],
                'expiresAt': claims['exp'],
              }),
            ),
          ]);
          continue;
        }
        final (clientId, bytes) = unwrapRelayFrame(raw);
        if (session == null) {
          final hello = decodeRelayJson(bytes);
          final ephemeral = await RelayIdentityKeyPair.generate();
          final nonce = decodeBase64Fixed(hello['nonce'], expectedLength: 16);
          session = await RelayCryptoSession.derive(
            localStatic: runtimeIdentity,
            localEphemeral: ephemeral,
            peerStatic: decodeBase64Fixed(hello['identityPublicKey']),
            peerEphemeral: decodeBase64Fixed(hello['ephemeralPublicKey']),
            runtimeId: runtimeId,
            clientId: clientId,
            nonce: nonce,
            initiator: false,
          );
          _replyBinary(
            socket,
            wrapRelayFrame(
              clientId,
              encodeRelayJson(<String, Object?>{
                'version': relayHelloVersion,
                'runtimeId': runtimeId,
                'clientId': clientId,
                'identityPublicKey': runtimePublicKey,
                'ephemeralPublicKey': base64UrlNoPadding(ephemeral.publicBytes),
                'nonce': hello['nonce'],
                'confirmation': base64UrlNoPadding(
                  await session.confirmation(),
                ),
              }),
            ),
          );
          continue;
        }
        if (!confirmed) {
          await session.verifyPeerConfirmation(
            decodeBase64Fixed(decodeRelayJson(bytes)['confirmation']),
          );
          confirmed = true;
          continue;
        }
        final envelope = fragments.accept(bytes);
        if (envelope == null) continue;
        final request = decodeRelayJson(await session.open(envelope));
        final payload = request['payload']! as Map<String, Object?>;
        Map<String, Object?> responsePayload = payload;
        if (request['type'] == 'mobile.relayAuthorization.renew') {
          renewalOrder.add('runtime');
          if (!runtimeRenewalStarted.isCompleted) {
            runtimeRenewalStarted.complete();
          }
          responsePayload = <String, Object?>{
            'expiresAt': _grantClaims(payload['grant'] as String)['exp'],
          };
        }
        if (request['type'] == 'mobile.hello') {
          hellos.add(payload);
        }
        final response = await session.seal(
          utf8.encode(
            jsonEncode(<String, Object?>{
              'id': request['id'],
              'ok': true,
              'payload': request['type'] == 'mobile.hello'
                  ? <String, Object?>{
                      'binaryFrames': true,
                      'runtimeCapabilities': <String>[
                        if (supportsRenewal) relayRenewalCapability,
                      ],
                    }
                  : responsePayload,
            }),
          ),
        );
        for (final fragment in fragmentRelayPayload(response)) {
          _replyBinary(socket, wrapRelayFrame(clientId, fragment));
        }
      }
    } on Object {
      if (socket.readyState == WebSocket.open) rethrow;
    } finally {
      session?.close();
    }
  }

  Future<void> dispose() async {
    await _subscription.cancel();
    for (final socket in _sockets) {
      await socket.close();
    }
    await Future.wait(_served);
    await _server.close(force: true);
  }
}

final class RelayCloudFixture
    implements CloudRelayIdentityRepository, AleraRelayCloudApi {
  RelayCloudFixture(
    this.gateway, {
    this.delayRenewal = false,
    this.initialGrantSeconds = 120,
  });

  final RelayRuntimeGateway gateway;
  final bool delayRenewal;
  final int initialGrantSeconds;
  int discoveryCalls = 0;
  int registrationCalls = 0;
  int grantCalls = 0;
  String? clientPublicKey;
  final Completer<void> renewalGrantRequested = Completer<void>();
  final Completer<CloudRelayGrant> _renewalGrant = Completer<CloudRelayGrant>();

  @override
  Future<String> getOrCreatePrivateKey(String accountId) async =>
      base64UrlNoPadding(List<int>.filled(32, 7));

  @override
  Future<List<CloudRuntimeProfile>> discoverRuntimes(
    CloudAccountSession session,
  ) async {
    discoveryCalls += 1;
    return <CloudRuntimeProfile>[
      CloudRuntimeProfile(
        id: gateway.runtimeId,
        name: 'Alera Host',
        lastSeenAt: DateTime.now().toUtc(),
        relayPublicKey: gateway.runtimePublicKey,
        relayKeyVersion: 1,
      ),
    ];
  }

  @override
  Future<CloudRelayIdentityRegistration> registerRelayIdentity({
    required CloudAccountSession session,
    required String publicKey,
    required int keyVersion,
  }) async {
    registrationCalls += 1;
    clientPublicKey = publicKey;
    return CloudRelayIdentityRegistration(
      clientId: 'cloud-installation-1',
      clientKind: 'mobile',
      publicKey: publicKey,
      keyVersion: keyVersion,
    );
  }

  @override
  Future<CloudRelayGrant> requestRelayGrant({
    required CloudAccountSession session,
    required String runtimeId,
  }) {
    grantCalls += 1;
    if (grantCalls > 1 && delayRenewal) {
      if (!renewalGrantRequested.isCompleted) {
        renewalGrantRequested.complete();
      }
      return _renewalGrant.future;
    }
    return Future<CloudRelayGrant>.value(_grant(initialGrantSeconds));
  }

  CloudRelayGrant _grant(int expiresIn) {
    final publicKey = clientPublicKey;
    if (publicKey == null) {
      throw StateError('Relay identity was not registered.');
    }
    final expiresAt = DateTime.now().millisecondsSinceEpoch ~/ 1000 + expiresIn;
    return CloudRelayGrant(
      grant: _grantToken(expiresAt),
      relayUrl: gateway.relayUrl,
      expiresIn: expiresIn,
      accountId: 'account-1',
      runtimeId: gateway.runtimeId,
      clientId: 'cloud-installation-1',
      clientKind: 'mobile',
      clientKeyVersion: 1,
      clientPublicKey: publicKey,
      runtimePublicKey: gateway.runtimePublicKey,
    );
  }

  void completeRenewal({int expiresIn = 120}) {
    if (!_renewalGrant.isCompleted) {
      _renewalGrant.complete(_grant(expiresIn));
    }
  }
}

Map<String, Object?> _grantClaims(String token) {
  final parts = token.split('.');
  return jsonDecode(
    utf8.decode(base64Url.decode(base64Url.normalize(parts[1]))),
  ) as Map<String, Object?>;
}

String _grantToken(int expiresAt) =>
    'header.${base64UrlNoPadding(utf8.encode(jsonEncode(<String, Object?>{'exp': expiresAt})))}.signature';

void _replyBinary(WebSocket socket, List<int> message) {
  try {
    socket.add(message);
  } on StateError catch (error) {
    if (error.message != 'StreamSink is closed') rethrow;
  }
}
