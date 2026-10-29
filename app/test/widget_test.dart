import 'package:flutter_test/flutter_test.dart';
import 'package:uft/direct.dart';
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

  test('invites round-trip and junk is ignored', () {
    final inv = Invite(const Wifi(ssid: 'uft-1a2b', pass: 'p4ss word', sec: 'wpa2'), 'ab' * 32, 'x' * 40);
    final back = Invite.parse(inv.toJson())!;
    expect([back.wifi.ssid, back.wifi.pass, back.wifi.sec, back.fp], ['uft-1a2b', 'p4ss word', 'wpa2', 'ab' * 32]);
    expect(back.name, 'x' * 32);
    expect(inv.toJson().length, lessThan(240));
    for (final junk in ['', 'null', '[]', '{"v":2}', '{"v":1,"ssid":3}']) {
      expect(Invite.parse(junk), isNull);
    }
  });
}
