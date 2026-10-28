import 'package:flutter_test/flutter_test.dart';
import 'package:uft/main.dart';

void main() {
  test('formats sizes and times', () {
    expect(size(0), '0 B');
    expect(size(1500), '1.5 KB');
    expect(size(2680000000), '2.7 GB');
    expect(speed(74.2e6), '74.2 MB/s');
    expect(duration(19), '0:19');
    expect(duration(3725), '1:02:05');
  });
}
