// Full flow against a `uft` CLI on the network: see app/test-e2e.sh.
// Without UFT_E2E_PEER it only checks that the app starts.
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:uft/main.dart';
import 'package:uft/src/rust/frb_generated.dart';

Future<void> waitFor(WidgetTester tester, Finder f, {int secs = 30}) async {
  for (var i = 0; i < secs * 10; i++) {
    await tester.pump(const Duration(milliseconds: 100));
    if (f.evaluate().isNotEmpty) return;
  }
  fail('timed out waiting for $f');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('finds the peer, sends to it, accepts from it', (tester) async {
    await tester.pumpWidget(const App());
    await waitFor(tester, find.text('Nearby devices'));
    final peer = Platform.environment['UFT_E2E_PEER'];
    if (peer == null) return;

    // Device appears in the list.
    await waitFor(tester, find.text(peer));
    final home = tester.state<HomeState>(find.byType(Home));
    final p = home.peers.firstWhere((x) => x.name == peer);

    // Send (the file picker is skipped; pick() hands its paths to send()).
    home.send(p, [Platform.environment['UFT_E2E_FILE']!]);
    await waitFor(tester, find.textContaining('Sent '), secs: 60);

    // The peer sends back: offer dialog with the code, accept it.
    await waitFor(tester, find.text('Accept'), secs: 60);
    expect(find.text(p.code), findsWidgets);
    await tester.tap(find.text('Accept'));
    await waitFor(tester, find.textContaining('Received '), secs: 60);
  });
}
