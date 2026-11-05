import 'package:flutter_test/flutter_test.dart';

import '../golden/alera_golden_harness.dart';

void main() {
  test(
    'CI golden comparator accepts only sparse image rasterization drift',
    () {
      expect(
        aleraCiGoldenDifferenceIsAcceptable(
          ComparisonResult(passed: true, diffPercent: 0),
        ),
        isTrue,
      );
      expect(
        aleraCiGoldenDifferenceIsAcceptable(
          ComparisonResult(
            passed: false,
            diffPercent: 0.00015,
            diffs: const {},
          ),
        ),
        isTrue,
      );
      expect(
        aleraCiGoldenDifferenceIsAcceptable(
          ComparisonResult(
            passed: false,
            diffPercent: 0.00021,
            diffs: const {},
          ),
        ),
        isFalse,
      );
      expect(
        aleraCiGoldenDifferenceIsAcceptable(
          ComparisonResult(passed: false, diffPercent: 0),
        ),
        isFalse,
        reason: 'Missing files/invalid diff evidence must never be ignored',
      );
      expect(
        aleraCiGoldenDifferenceIsAcceptable(
          ComparisonResult(passed: false, diffPercent: 1, diffs: const {}),
        ),
        isFalse,
        reason: 'Changed dimensions must fail even in CI',
      );
    },
  );
}
