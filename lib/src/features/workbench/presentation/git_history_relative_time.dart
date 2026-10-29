/// Compact commit age shown next to a Git history row.
///
/// [now] is injectable so clock boundaries and future timestamps can be tested
/// without pumping a widget or depending on a process-global clock.
String gitHistoryRelativeTime(DateTime? timestamp, {DateTime? now}) {
  if (timestamp == null) {
    return '';
  }
  final difference = (now ?? DateTime.now()).difference(timestamp.toLocal());
  if (difference.isNegative) {
    return 'just now';
  }
  if (difference.inMinutes < 1) {
    return 'just now';
  }
  if (difference.inHours < 1) {
    return '${difference.inMinutes}m ago';
  }
  if (difference.inDays < 1) {
    return '${difference.inHours}h ago';
  }
  if (difference.inDays < 30) {
    return '${difference.inDays}d ago';
  }
  final local = timestamp.toLocal();
  final month = local.month.toString().padLeft(2, '0');
  final day = local.day.toString().padLeft(2, '0');
  return '${local.year}-$month-$day';
}
