// Full flow against a `swoop` CLI on the network: see app/test-e2e.sh (Linux) and app/test-android.sh.
// Without UFT_E2E_PEER (environment, or --dart-define on a device) it only checks that the app starts.
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path_provider/path_provider.dart';
import 'package:uft/main.dart';
import 'package:uft/src/rust/api/uft.dart' show Pick;
import 'package:uft/src/rust/frb_generated.dart';

String? setting(String key) {
  final defined = switch (key) {
    'UFT_E2E_PEER' => const String.fromEnvironment('UFT_E2E_PEER'),
    'UFT_E2E_FILE' => const String.fromEnvironment('UFT_E2E_FILE'),
    'UFT_E2E_ADDR' => const String.fromEnvironment('UFT_E2E_ADDR'),
    _ => '',
  };
  return defined.isNotEmpty ? defined : Platform.environment[key];
}

/// Pumps until `f` shows up; on timeout fails with `why()` (e.g. the transfers' results).
Future<void> waitFor(
  WidgetTester tester,
  Finder f, {
  int secs = 30,
  List<Transfer> Function()? why,
}) async {
  for (var i = 0; i < secs * 10; i++) {
    await tester.pump(const Duration(milliseconds: 100));
    if (f.evaluate().isNotEmpty) return;
  }
  fail(
    'timed out waiting for $f${why == null ? '' : ': ${why().map((t) => '${t.title}: ${t.result ?? '${t.done}/${t.total} bytes'}')}'}',
  );
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('finds the peer, sends to it, accepts from it', (tester) async {
    await tester.pumpWidget(const App());
    await waitFor(tester, find.text('Nearby devices'));
    final peer = setting('UFT_E2E_PEER');
    if (peer == null) return;
    final home = tester.state<HomeState>(find.byType(Home));
    // For the script: the CLI needs this to send back where it can't scan (an emulator behind adb forward).
    await waitFor(tester, find.textContaining('saving to'));
    final config = await getApplicationSupportDirectory();
    File('${config.path}/e2e-fp').writeAsStringSync(home.me!.fingerprint);

    // Device appears in the list: found by scanning, or asked at UFT_E2E_ADDR (an emulator can't scan the host).
    final addr = setting('UFT_E2E_ADDR');
    if (addr != null) await home.addPeer(addr);
    await waitFor(tester, find.text(peer), secs: 60);
    final p = [...home.peers, ...home.manual].firstWhere((x) => x.name == peer);

    // Send (the file picker is skipped; pick() hands its picks to send()).
    var file = setting('UFT_E2E_FILE');
    if (file == null) {
      final r = Random(1);
      file = '${(await getTemporaryDirectory()).path}/out.bin';
      File(file).writeAsBytesSync(
        Uint8List(64 << 20)..setAll(0, Iterable.generate(64 << 20, (_) => r.nextInt(256))),
      );
    }
    home.send(p, [Pick(path: file)]);
    await waitFor(tester, find.textContaining('Sent '), secs: 120, why: () => home.transfers);

    // The peer sends back: offer dialog with the code, accept it.
    await waitFor(tester, find.text('Accept'), secs: 120);
    expect(find.textContaining(RegExp(r'^\d{3} \d{3}$')), findsOneWidget);
    await tester.tap(find.text('Accept'));
    await waitFor(tester, find.textContaining('Received '), secs: 120, why: () => home.transfers);
  });
}
