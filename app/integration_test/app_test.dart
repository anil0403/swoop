import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:uft/main.dart';
import 'package:uft/src/rust/frb_generated.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());
  testWidgets('starts and shows the device list', (tester) async {
    await tester.pumpWidget(const App());
    await tester.pump(const Duration(seconds: 1));
    expect(find.text('Nearby devices'), findsOneWidget);
  });
}
