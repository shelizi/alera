/// Local-only, in-memory retry state for an explicitly confirmed historical
/// Agent Resume. A lost acknowledgement MUST NOT turn the next deliberate
/// attempt into a new Host mutation: the Host can replay the same durable
/// receipt only when the source identity and mutation UUID are unchanged.
///
/// This is not a background retry or a persistence mechanism. The caller must
/// still require two explicit clicks and inspect Live Agents after an
/// ambiguous outcome. New identities get independent mutation UUIDs.
typedef ExactHistoryResumeIdentity = ({
  String provider,
  String accountId,
  String projectId,
  String workspaceId,
  String providerSessionId,
  String profileId,
});

class ExactHistoryResumeRetryTracker {
  final Map<ExactHistoryResumeIdentity, String> _unconfirmed = {};

  bool hasUnconfirmed(ExactHistoryResumeIdentity identity) =>
      _unconfirmed.containsKey(identity);

  /// Reserve the UUID before issuing the request. Reuses it after an error or
  /// lost reply; only a *confirmed* Host response clears this association.
  String reserve(
    ExactHistoryResumeIdentity identity,
    String Function() createMutationId,
  ) => _unconfirmed.putIfAbsent(identity, createMutationId);

  void acknowledge(ExactHistoryResumeIdentity identity) =>
      _unconfirmed.remove(identity);
}
