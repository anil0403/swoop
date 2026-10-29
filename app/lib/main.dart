import 'dart:async';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart' show AnyhowException;
import 'package:path_provider/path_provider.dart';
import 'package:uft/src/rust/api/uft.dart' as uft;
import 'package:uft/src/rust/frb_generated.dart';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:qr_flutter/qr_flutter.dart';

import 'direct.dart' as direct;

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(const App());
}

class App extends StatelessWidget {
  const App({super.key});

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'Swoop',
    theme: theme(Brightness.light),
    darkTheme: theme(Brightness.dark),
    home: const Home(),
  );
}

/// The icon's blue, flat cards with rounded corners, the rounded progress bar.
ThemeData theme(Brightness b) {
  final t = ThemeData(colorSchemeSeed: const Color(0xFF1D5FD1), brightness: b);
  final c = t.colorScheme;
  return t.copyWith(
    cardTheme: CardThemeData(
      elevation: 0,
      margin: const EdgeInsets.symmetric(vertical: 4),
      color: c.surfaceContainerLow,
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(
      linearMinHeight: 6,
      borderRadius: BorderRadius.circular(3),
      linearTrackColor: c.surfaceContainerHighest,
    ),
    appBarTheme: const AppBarTheme(centerTitle: false),
  );
}

const tabular = [FontFeature.tabularFigures()];

/// Fades and lifts a list item in the first time it's built; key it so it stays with its item.
class Appear extends StatelessWidget {
  const Appear({super.key, required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context) => TweenAnimationBuilder<double>(
    tween: Tween(begin: 0, end: 1),
    duration: const Duration(milliseconds: 350),
    curve: Curves.easeOutCubic,
    builder: (context, v, child) => Opacity(
      opacity: v,
      child: Transform.translate(offset: Offset(0, 12 * (1 - v)), child: child),
    ),
    child: child,
  );
}

/// Rings spreading out from `child`: this device looking for others.
class Pulse extends StatefulWidget {
  const Pulse({super.key, required this.size, required this.child});
  final double size;
  final Widget child;

  @override
  State<Pulse> createState() => _PulseState();
}

class _PulseState extends State<Pulse> with SingleTickerProviderStateMixin {
  late final c = AnimationController(vsync: this, duration: const Duration(milliseconds: 2400))
    ..repeat();

  @override
  void dispose() {
    c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => SizedBox.square(
    dimension: widget.size,
    child: MediaQuery.disableAnimationsOf(context)
        ? Center(child: widget.child)
        : CustomPaint(
            painter: _Rings(c, Theme.of(context).colorScheme.primary),
            child: Center(child: widget.child),
          ),
  );
}

class _Rings extends CustomPainter {
  _Rings(this.a, this.color) : super(repaint: a);
  final Animation<double> a;
  final Color color;

  @override
  void paint(Canvas canvas, Size s) {
    final r = s.shortestSide / 2;
    for (final offset in [0.0, 0.5]) {
      final t = (a.value + offset) % 1;
      canvas.drawCircle(
        s.center(Offset.zero),
        r * (0.35 + 0.65 * Curves.easeOut.transform(t)),
        Paint()..color = color.withValues(alpha: 0.22 * (1 - t)),
      );
    }
  }

  @override
  bool shouldRepaint(_Rings old) => old.color != color;
}

/// One transfer card, sending or receiving.
class Transfer {
  Transfer(this.title, {this.total = 0, this.sending = true});
  final String title;
  final bool sending;
  int done = 0, total;
  double speed = 0, peak = 0;
  double? eta;

  /// What it's waiting for, before bytes move.
  String? note;
  String? result;
  bool failed = false;

  /// A running send's id, for `uft.cancel`.
  int? sendId;
  final started = DateTime.now();
}

var _nextSend = 1;

/// Android side (MainActivity.kt): device name, save folder, file picking, power locks.
const android = MethodChannel('uft');

String size(num b) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  var i = 0;
  var v = b.toDouble();
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return '${v.toStringAsFixed(i == 0 ? 0 : 1)} ${units[i]}';
}

/// An error's text, without the bridge's wrapper.
String msg(Object e) => e is AnyhowException ? e.message : '$e';

String speed(double bps) => '${(bps / 1e6).toStringAsFixed(1)} MB/s';

String duration(double s) {
  final t = s.round();
  return t >= 3600
      ? '${t ~/ 3600}:${(t ~/ 60 % 60).toString().padLeft(2, '0')}:${(t % 60).toString().padLeft(2, '0')}'
      : '${t ~/ 60}:${(t % 60).toString().padLeft(2, '0')}';
}

class Home extends StatefulWidget {
  const Home({super.key});

  @override
  State<Home> createState() => HomeState();
}

class HomeState extends State<Home> {
  uft.Me? me;
  String? dir, error;
  List<uft.Peer> peers = [];

  /// Added by address or link; kept across rescans.
  final manual = <uft.Peer>[];
  final transfers = <Transfer>[];
  final incoming = <int, Transfer>{};
  bool busy = false, receiving = true, dragging = false;

  /// False once receiving failed to start, e.g. another copy of Swoop has the port.
  bool listening = true;

  /// Shared to Swoop (Android) or dropped on the window (Linux): sent to the next device tapped.
  List<uft.Pick>? pending;

  /// Transfer progress into the Android notification while busy.
  Timer? notify;
  Timer? rescan;

  /// Seen over Bluetooth, by fingerprint prefix; those not also on this network get the direct link.
  final ble = <String, direct.BlePeer>{};
  String? bleError;

  /// Senders whose invite this device accepted: their offer needs no second tap.
  final invited = <String>{};

  /// The sender this device joined a hotspot for, and its transfer; the network is left when it ends.
  String? joinedFor;
  int? joinedSession;
  Timer? leaveTimer;

  @override
  void initState() {
    super.initState();
    start();
  }

  @override
  void dispose() {
    rescan?.cancel();
    notify?.cancel();
    leaveTimer?.cancel();
    super.dispose();
  }

  Future<void> start() async {
    try {
      final config = await getApplicationSupportDirectory();
      final String d, name;
      if (Platform.isAndroid) {
        final info = (await android.invokeMapMethod<String, String>('info'))!;
        (d, name) = (info['downloads']!, info['name']!);
      } else {
        final downloads = await getDownloadsDirectory() ?? await getApplicationDocumentsDirectory();
        (d, name) = ('${downloads.path}/Swoop', Platform.localHostname);
      }
      final m = await uft.init(configDir: config.path, name: name);
      setState(() {
        me = m;
        dir = d;
      });
      uft
          .receive(dir: d)
          .listen(
            onIncoming,
            onError: (Object e) => setState(() {
              error = msg(e);
              listening = false;
            }),
          );
      scan();
      startBle();
      if (Platform.isAndroid) {
        android.setMethodCallHandler((c) async {
          if (c.method == 'intent') opened();
        });
        opened();
      }
    } catch (e) {
      setState(() => error = msg(e));
    }
  }

  /// Scans for a few seconds, then again shortly after; devices that stop answering drop off.
  void scan() {
    if (!mounted) return;
    var latest = <uft.Peer>[];
    uft
        .scan(secs: 3)
        .listen(
          (list) => setState(() {
            latest = list;
            for (final p in list) {
              final i = peers.indexWhere((x) => x.fingerprint == p.fingerprint);
              i < 0 ? peers.add(p) : peers[i] = p;
            }
          }),
          onError: (Object e) => setState(() => error = msg(e)),
          onDone: () {
            if (!mounted) return;
            final stale = DateTime.now().subtract(const Duration(seconds: 15));
            setState(() {
              peers = latest;
              ble.removeWhere((_, b) => b.seen.isBefore(stale));
            });
            // Bluetooth turned on or permission granted since: try again.
            if (bleError != null) startBle();
            if (Platform.isAndroid) {
              android
                  .invokeMapMethod<String, Object?>('link')
                  .then(
                    (l) => uft.setLink(
                      kind: l?['kind'] as String?,
                      mhz: l?['mhz'] as int?,
                      mbps: l?['mbps'] as int?,
                      dbm: l?['dbm'] as int?,
                    ),
                  );
            }
            rescan = Timer(const Duration(seconds: 4), scan);
          },
        );
  }

  /// Files shared to Swoop, and swoop:// links opened (a QR code scanned with the camera).
  Future<void> opened() async {
    final got = await android.invokeListMethod<List<Object?>>('shared') ?? [];
    if (got.isNotEmpty) {
      if (pending != null) release(pending!);
      setState(
        () => pending = [for (final x in got) uft.Pick(path: x[0] as String, name: x[1] as String)],
      );
    }
    final link = await android.invokeMethod<String>('openedLink');
    if (link == null) return;
    try {
      await addPeer(link);
    } catch (e) {
      setState(() => error = msg(e));
    }
  }

  List<uft.Pick>? takePending() {
    final x = pending;
    if (x != null) setState(() => pending = null);
    return x;
  }

  void startBle() {
    bleError = null;
    direct.bleStart(
      me!.fingerprint.substring(0, 16),
      me!.name,
      // Keeps the name when an advertisement comes without one.
      seen: (b) => setState(
        () => ble[b.fp] = b.name.isEmpty && ble[b.fp] != null ? (ble[b.fp]!..seen = b.seen) : b,
      ),
      invite: onInvite,
      failed: (e) => setState(() => bleError = e),
    );
  }

  void onIncoming(uft.Incoming e) {
    switch (e) {
      case uft.Incoming_Offer():
        // Accepted with the invite already; TLS proves it's from the device that sent it.
        invited.remove(e.fingerprint)
            ? uft.answer(id: e.id, accept: true, always: false)
            : offer(e);
      case uft.Incoming_Started():
        if (e.fingerprint == joinedFor) {
          joinedSession = e.session;
          leaveTimer?.cancel();
        }
        final t = Transfer(
          'From ${e.peerName} · ${e.files} file${e.files == 1 ? '' : 's'}',
          total: e.total,
          sending: false,
        )..done = e.resumed;
        setState(() => transfers.insert(0, incoming[e.session] = t));
      case uft.Incoming_Progress():
        final t = incoming[e.session];
        if (t != null) {
          setState(() {
            t.done = e.done;
            t.total = e.total;
            t.speed = e.speed;
            if (e.speed > t.peak) t.peak = e.speed;
            t.eta = e.eta;
          });
        }
      case uft.Incoming_Done():
        if (e.session == joinedSession) leaveDirect();
        final t = incoming.remove(e.session);
        if (t != null) {
          setState(() {
            t.done = t.total;
            t.result = 'Received ${size(e.bytes)} in ${duration(e.secs)} · saved to $shortDir';
          });
          // Photos and videos show up in the Gallery.
          if (Platform.isAndroid) {
            android.invokeMethod('scan', {'dir': dir, 'since': t.started.millisecondsSinceEpoch});
          }
        }
      case uft.Incoming_Failed():
        if (e.session == joinedSession) leaveDirect();
        final t = incoming.remove(e.session);
        if (t != null) {
          setState(
            () => t
              ..failed = true
              ..result = e.error,
          );
        }
    }
    updateBusy();
  }

  Future<void> offer(uft.Incoming_Offer o) async {
    final choice = await showDialog<(bool, bool)>(
      context: context,
      barrierDismissible: false,
      builder: (context) => AlertDialog(
        icon: const Icon(Icons.download_rounded),
        title: Text('${o.peerName} wants to send you ${o.files} file${o.files == 1 ? '' : 's'}'),
        scrollable: true,
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(size(o.total), style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 24),
            const Text(
              'Check that the other device shows the same code:',
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 12),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 12),
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.primaryContainer,
                borderRadius: BorderRadius.circular(16),
              ),
              child: Text(
                o.code,
                style: Theme.of(context).textTheme.headlineMedium?.copyWith(
                  color: Theme.of(context).colorScheme.onPrimaryContainer,
                  fontWeight: FontWeight.w600,
                  letterSpacing: 4,
                  fontFeatures: tabular,
                ),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, (false, false)),
            child: const Text('Decline'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, (true, true)),
            child: const Text('Always accept'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, (true, false)),
            child: const Text('Accept'),
          ),
        ],
      ),
    );
    final (accept, always) = choice ?? (false, false);
    uft.answer(id: o.id, accept: accept, always: always);
  }

  Future<void> pick(uft.Peer p) async {
    final picks = takePending() ?? await choose('Send to ${p.name} · code ${p.code}');
    if (picks.isNotEmpty) send(p, picks);
  }

  /// Files or a folder, from the platform's picker.
  Future<List<uft.Pick>> choose(String title) async {
    final folder = await showModalBottomSheet<bool>(
      context: context,
      showDragHandle: true,
      builder: (context) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(24, 0, 24, 8),
              child: Text(title, style: Theme.of(context).textTheme.titleMedium),
            ),
            ListTile(
              contentPadding: const EdgeInsets.symmetric(horizontal: 24),
              leading: const Icon(Icons.description),
              title: const Text('Files'),
              onTap: () => Navigator.pop(context, false),
            ),
            ListTile(
              contentPadding: const EdgeInsets.symmetric(horizontal: 24),
              leading: const Icon(Icons.folder),
              title: const Text('Folder'),
              onTap: () => Navigator.pop(context, true),
            ),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
    if (folder == null) return [];
    if (Platform.isAndroid) {
      // [fd path, name] pairs; the descriptors stay open until the send ends.
      final got = await android.invokeListMethod<List<Object?>>('pick', folder) ?? [];
      return [for (final x in got) uft.Pick(path: x[0] as String, name: x[1] as String)];
    }
    final paths = folder
        ? [?await getDirectoryPath()]
        : (await openFiles()).map((f) => f.path).toList();
    return [for (final x in paths) uft.Pick(path: x)];
  }

  /// Tells Android while something transfers, so it keeps the CPU and Wi-Fi at full speed.
  void updateBusy() {
    final now = transfers.any((t) => t.result == null);
    if (now != busy && Platform.isAndroid) {
      android.invokeMethod('busy', now);
      notify?.cancel();
      notify = now ? Timer.periodic(const Duration(seconds: 1), (_) => showProgress()) : null;
    }
    busy = now;
  }

  void showProgress() {
    final running = transfers.where((t) => t.result == null && t.note == null).toList();
    android.invokeMethod(
      'progress',
      running.isEmpty
          ? null
          : {
              'title': running.length == 1 ? running.first.title : '${running.length} transfers',
              'done': running.fold(0, (a, t) => a + t.done),
              'total': running.fold(0, (a, t) => a + t.total),
            },
    );
  }

  /// "file.txt", "folder" or "3 items".
  String describe(List<uft.Pick> picks) {
    final first = (picks.first.name ?? picks.first.path).split('/');
    return picks.length == 1 || picks.every((x) => x.name?.startsWith('${first[0]}/') ?? false)
        ? (picks.length == 1 ? first.last : first[0])
        : '${picks.length} items';
  }

  void release(List<uft.Pick> picks) {
    if (Platform.isAndroid) android.invokeMethod('release', [for (final x in picks) x.path]);
  }

  /// Sends into `card` if given (a direct link set up first), then calls `after`.
  void send(uft.Peer p, List<uft.Pick> picks, {Transfer? card, void Function()? after}) {
    final t = card ?? Transfer('To ${p.name} · ${describe(picks)}');
    t
      ..note = null
      ..sendId = _nextSend++;
    if (card == null) setState(() => transfers.insert(0, t));
    updateBusy();
    void end() {
      setState(() => t.sendId = null);
      updateBusy();
      release(picks);
      after?.call();
    }

    uft
        .sendFiles(id: t.sendId!, peer: p, picks: picks)
        .listen(
          (e) => setState(() {
            switch (e) {
              case uft.Outgoing_Progress():
                t
                  ..note = null
                  ..done = e.done
                  ..total = e.total
                  ..speed = e.speed
                  ..peak = e.peak
                  ..eta = e.eta;
              case uft.Outgoing_Done():
                t
                  ..done = t.total
                  ..result =
                      'Sent ${size(e.bytes)} in ${duration(e.secs)} · '
                      'avg ${speed(e.avg)} · peak ${speed(e.peak)}'
                      '${e.resumed > 0 ? ' · ${size(e.resumed)} already there' : ''}'
                      '${[?uft.diagnose(fingerprint: p.fingerprint, speed: e.avg)].map((d) => '\n$d').join()}';
              case uft.Outgoing_Reconnecting():
                t.note = 'Connection lost (${e.error}); looking for ${p.name} again…';
              case uft.Outgoing_Failed():
                t
                  ..failed = true
                  ..note = null
                  ..result = e.error == 'cancelled'
                      ? 'Cancelled · send the same files again to continue where it stopped'
                      : e.error;
            }
          }),
          onDone: end,
          onError: (Object e) => setState(
            () => t
              ..failed = true
              ..result = msg(e),
          ),
        );
  }

  /// To a device seen only over Bluetooth: host a hotspot, invite it over Bluetooth, wait until it's on the hotspot,
  /// send, take the hotspot down.
  Future<void> sendDirect(direct.BlePeer b) async {
    final picks = takePending() ?? await choose('Send to ${b.name} · direct Wi-Fi link');
    if (picks.isEmpty) return;
    final t = Transfer('To ${b.name} · ${describe(picks)}')..note = 'Starting a hotspot…';
    setState(() => transfers.insert(0, t));
    updateBusy();
    var hosting = false;
    try {
      final wifi = await direct.host();
      hosting = true;
      // Linux peers may ask to pair first (BlueZ reads PipeWire's MIDI service on connect).
      setState(
        () => t.note =
            'Inviting ${b.name} over Bluetooth…${Platform.isLinux ? ' Accept a pairing request if one shows up.' : ''}',
      );
      await direct.bleInvite(b.id, direct.Invite(wifi, me!.fingerprint, me!.name).toJson());
      setState(() => t.note = 'Waiting for ${b.name} to accept and join ${wifi.ssid}…');
      // The receiver taps once or twice (Android 10+ adds a system dialog), then joins; the scan finds it.
      final until = DateTime.now().add(const Duration(minutes: 2));
      uft.Peer? p;
      while ((p = peers.where((x) => x.fingerprint.startsWith(b.fp)).firstOrNull) == null) {
        if (DateTime.now().isAfter(until)) throw '${b.name} did not join the hotspot';
        await Future<void>.delayed(const Duration(seconds: 1));
      }
      send(p!, picks, card: t, after: direct.stopHost);
    } catch (e) {
      if (hosting) direct.stopHost();
      release(picks);
      setState(
        () => t
          ..failed = true
          ..note = null
          ..result = msg(e),
      );
      updateBusy();
    }
  }

  /// A sender with no shared network asks this device to join its hotspot; accepting also accepts its files.
  Future<void> onInvite(String json) async {
    final inv = direct.Invite.parse(json);
    if (inv == null || joinedFor != null || !mounted) return;
    final ok = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (context) => AlertDialog(
        icon: const Icon(Icons.wifi_tethering),
        title: Text('${inv.name} wants to send you files'),
        content: Text(
          "You're not on the same network, so this device joins ${inv.name}'s hotspot (${inv.wifi.ssid}) "
          'for the transfer and goes back afterwards.',
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Decline')),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: const Text('Accept')),
        ],
      ),
    );
    if (ok != true) return;
    joinedFor = inv.fp;
    invited.add(inv.fp);
    try {
      await direct.join(inv.wifi);
      // Back to the usual network if the transfer never starts.
      leaveTimer = Timer(const Duration(minutes: 2), leaveDirect);
    } catch (e) {
      invited.remove(inv.fp);
      joinedFor = null;
      setState(() => error = "Couldn't join ${inv.name}'s hotspot: ${msg(e)}");
    }
  }

  void leaveDirect() {
    leaveTimer?.cancel();
    if (joinedFor != null) invited.remove(joinedFor);
    joinedFor = joinedSession = null;
    direct.leave();
  }

  /// For networks where scanning can't reach: shows this device's links, takes the other one's.
  Future<void> connectByAddress() async {
    final field = TextEditingController();
    final links = uft.myLinks();
    final target = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Connect by address'),
        scrollable: true,
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('On the other device, type one of these:'),
            const SizedBox(height: 8),
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.surfaceContainerHighest,
                borderRadius: BorderRadius.circular(12),
              ),
              child: SelectableText(
                links.isEmpty ? 'No network address yet' : links.join('\n'),
                style: const TextStyle(fontFamily: 'monospace', fontSize: 13, height: 1.6),
              ),
            ),
            // A phone's camera opens Swoop from it.
            if (!Platform.isAndroid && links.isNotEmpty) ...[
              const SizedBox(height: 20),
              const Text('Or scan this with the phone camera:'),
              const SizedBox(height: 12),
              Center(
                child: Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: Colors.white,
                    borderRadius: BorderRadius.circular(16),
                  ),
                  child: QrImageView(data: links.first, size: 168, padding: EdgeInsets.zero),
                ),
              ),
            ],
            const SizedBox(height: 24),
            TextField(
              controller: field,
              autofocus: true,
              decoration: const InputDecoration(
                labelText: "The other device's address or swoop:// link",
                hintText: '192.168.1.20',
              ),
              onSubmitted: (v) => Navigator.pop(context, v),
            ),
          ],
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context), child: const Text('Cancel')),
          FilledButton(
            onPressed: () => Navigator.pop(context, field.text),
            child: const Text('Connect'),
          ),
        ],
      ),
    );
    if (target == null || target.trim().isEmpty) return;
    try {
      await addPeer(target);
    } catch (e) {
      setState(() => error = msg(e));
    }
  }

  /// Adds the device at `target` (an address or a uft:// link) to the list.
  Future<uft.Peer> addPeer(String target) async {
    final p = await uft.connect(target: target);
    setState(() {
      error = null;
      manual
        ..removeWhere((m) => m.fingerprint == p.fingerprint)
        ..add(p);
    });
    return p;
  }

  /// "~/Downloads/Swoop" rather than the full home path.
  String get shortDir {
    final home = Platform.environment['HOME'];
    return home != null && dir!.startsWith('$home/') ? '~${dir!.substring(home.length)}' : dir!;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final c = theme.colorScheme;
    final devices = [
      for (final p in [
        ...peers,
        ...manual.where((m) => !peers.any((p) => p.fingerprint == m.fingerprint)),
      ])
        device(
          ValueKey('p${p.fingerprint}'),
          icon: p.os == 'android' ? Icons.phone_android : Icons.computer,
          name: p.name,
          detail:
              '${p.addr} · code ${p.code}'
              '${p.link == null ? '' : '\n${p.link}'}'
              '${p.expected == null ? '' : ' · up to ${speed(p.expected!)}'}',
          onTap: () => pick(p),
        ),
      for (final b in ble.values.where(
        (b) => ![...peers, ...manual].any((p) => p.fingerprint.startsWith(b.fp)),
      ))
        device(
          ValueKey('b${b.fp}'),
          icon: Icons.bluetooth,
          name: b.name.isEmpty ? 'Swoop device' : b.name,
          detail: 'Not on this network · sends over a direct Wi-Fi link',
          onTap: () => sendDirect(b),
        ),
    ];
    final top = [
      AnimatedSize(
        duration: const Duration(milliseconds: 250),
        curve: Curves.easeOutCubic,
        alignment: Alignment.topCenter,
        child: Column(
          children: [
            if (error != null)
              Appear(
                key: ValueKey('e$error'),
                child: banner(
                  color: c.errorContainer,
                  on: c.onErrorContainer,
                  icon: Icons.error_outline,
                  title: error!,
                  close: 'Dismiss',
                  onClose: () => setState(() => error = null),
                ),
              ),
            if (pending != null)
              Appear(
                key: const ValueKey('pending'),
                child: banner(
                  color: c.primaryContainer,
                  on: c.onPrimaryContainer,
                  icon: Icons.send_rounded,
                  title: 'Send ${describe(pending!)}',
                  subtitle: 'Tap a device below',
                  close: 'Cancel',
                  onClose: () => release(takePending()!),
                ),
              ),
          ],
        ),
      ),
      status(theme),
    ];
    final left = [
      header(
        theme,
        'Nearby devices',
        trailing: me == null
            ? null
            : Pulse(size: 20, child: CircleAvatar(radius: 3, backgroundColor: c.primary)),
      ),
      if (devices.isEmpty) empty(theme) else ...devices,
      if (bleError != null)
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 12, 4, 0),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.bluetooth_disabled, size: 16, color: c.onSurfaceVariant),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  'Bluetooth: $bleError. Devices on other networks only show up with Bluetooth on.',
                  style: theme.textTheme.bodySmall?.copyWith(color: c.onSurfaceVariant),
                ),
              ),
            ],
          ),
        ),
    ];
    final finished = transfers.any((t) => t.result != null);
    final right = [
      header(
        theme,
        'Transfers',
        action: finished
            ? TextButton(
                onPressed: () => setState(() => transfers.removeWhere((t) => t.result != null)),
                child: const Text('Clear finished'),
              )
            : null,
      ),
      for (final t in transfers) Appear(key: ObjectKey(t), child: TransferCard(t)),
    ];
    return Scaffold(
      appBar: AppBar(
        title: const Text('Swoop'),
        actions: [
          IconButton(
            icon: const Icon(Icons.add_link),
            tooltip: 'Connect by address',
            onPressed: me == null ? null : connectByAddress,
          ),
          const SizedBox(width: 8),
        ],
      ),
      body: dropZone(
        theme,
        SafeArea(
          top: false,
          child: LayoutBuilder(
            builder: (context, box) {
              // Two columns on a wide window: devices on the left, transfers on the right.
              if (box.maxWidth >= 900) {
                final side = (box.maxWidth - 1200).clamp(0, double.infinity) / 2 + 24;
                return Column(
                  children: [
                    Padding(
                      padding: EdgeInsets.fromLTRB(side, 8, side, 0),
                      child: Column(children: top),
                    ),
                    Expanded(
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Expanded(
                            child: ListView(
                              padding: EdgeInsets.fromLTRB(side, 0, 12, 24),
                              children: left,
                            ),
                          ),
                          Expanded(
                            child: ListView(
                              padding: EdgeInsets.fromLTRB(12, 0, side, 24),
                              children: [...right, if (transfers.isEmpty) hint(theme)],
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                );
              }
              // One column, kept readable on a medium window.
              final side = (box.maxWidth - 640).clamp(0, double.infinity) / 2 + 16;
              return ListView(
                padding: EdgeInsets.fromLTRB(side, 8, side, 24),
                children: [...top, ...left, if (transfers.isNotEmpty) ...right],
              );
            },
          ),
        ),
      ),
    );
  }

  /// This device: its name, whether it takes offers, where files go.
  Widget status(ThemeData theme) {
    final c = theme.colorScheme;
    return Card(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 12, 12),
        child: Row(
          children: [
            AnimatedContainer(
              duration: const Duration(milliseconds: 250),
              width: 40,
              height: 40,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: receiving && listening ? c.primary : c.surfaceContainerHighest,
              ),
              child: Icon(
                Platform.isAndroid ? Icons.phone_android : Icons.computer,
                size: 20,
                color: receiving && listening ? c.onPrimary : c.onSurfaceVariant,
              ),
            ),
            const SizedBox(width: 16),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(me?.name ?? 'Starting…', style: theme.textTheme.titleMedium),
                  const SizedBox(height: 2),
                  AnimatedSwitcher(
                    duration: const Duration(milliseconds: 200),
                    child: Text(
                      key: ValueKey((receiving, listening)),
                      dir == null
                          ? 'Getting ready'
                          : !listening
                          ? "Can't receive files right now"
                          : receiving
                          ? 'Visible to nearby devices · saving to $shortDir'
                          : 'Not receiving · offers are declined',
                      style: theme.textTheme.bodySmall?.copyWith(color: c.onSurfaceVariant),
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(width: 8),
            Tooltip(
              message: 'Receive files',
              child: Switch(
                value: receiving && listening,
                onChanged: me == null || !listening
                    ? null
                    : (v) {
                        setState(() => receiving = v);
                        uft.setReceiving(enabled: v);
                      },
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget header(ThemeData theme, String text, {Widget? trailing, Widget? action}) => Padding(
    padding: const EdgeInsets.fromLTRB(4, 24, 0, 8),
    child: SizedBox(
      height: 32,
      child: Row(
        children: [
          Text(
            text,
            style: theme.textTheme.titleSmall?.copyWith(
              color: theme.colorScheme.primary,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(width: 4),
          ?trailing,
          if (action != null) ...[const Spacer(), action],
        ],
      ),
    ),
  );

  Widget device(
    Key key, {
    required IconData icon,
    required String name,
    required String detail,
    required VoidCallback onTap,
  }) {
    final c = Theme.of(context).colorScheme;
    return Appear(
      key: key,
      child: Card(
        child: ListTile(
          contentPadding: const EdgeInsets.fromLTRB(16, 6, 12, 6),
          leading: CircleAvatar(
            backgroundColor: c.secondaryContainer,
            foregroundColor: c.onSecondaryContainer,
            child: Icon(icon, size: 20),
          ),
          title: Text(name, maxLines: 1, overflow: TextOverflow.ellipsis),
          subtitle: Text(detail),
          trailing: Icon(Icons.send_rounded, color: c.primary),
          onTap: onTap,
        ),
      ),
    );
  }

  Widget banner({
    required Color color,
    required Color on,
    required IconData icon,
    required String title,
    String? subtitle,
    required String close,
    required VoidCallback onClose,
  }) => Card(
    color: color,
    margin: const EdgeInsets.only(bottom: 8),
    child: ListTile(
      iconColor: on,
      textColor: on,
      leading: Icon(icon),
      title: Text(title),
      subtitle: subtitle == null ? null : Text(subtitle),
      trailing: IconButton(icon: const Icon(Icons.close), tooltip: close, onPressed: onClose),
    ),
  );

  /// No devices yet: what's happening and what to do.
  Widget empty(ThemeData theme) {
    final c = theme.colorScheme;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 24),
      child: Column(
        children: [
          Pulse(
            size: 128,
            child: CircleAvatar(
              radius: 28,
              backgroundColor: c.primaryContainer,
              foregroundColor: c.onPrimaryContainer,
              child: const Icon(Icons.wifi_tethering),
            ),
          ),
          const SizedBox(height: 16),
          Text(
            'Looking for devices on this network…',
            style: theme.textTheme.titleMedium,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 4),
          Text(
            'Open Swoop on the other device.',
            style: theme.textTheme.bodyMedium?.copyWith(color: c.onSurfaceVariant),
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 20),
          OutlinedButton.icon(
            onPressed: me == null ? null : connectByAddress,
            icon: const Icon(Icons.add_link),
            label: const Text('Connect by address'),
          ),
        ],
      ),
    );
  }

  /// The empty transfers column on a wide window.
  Widget hint(ThemeData theme) => Padding(
    padding: const EdgeInsets.fromLTRB(4, 8, 4, 0),
    child: Text(
      Platform.isLinux
          ? 'Nothing sent yet. Tap a device to pick files, or drop files on this window.'
          : 'Nothing sent yet. Tap a device to pick files.',
      style: theme.textTheme.bodyMedium?.copyWith(color: theme.colorScheme.onSurfaceVariant),
    ),
  );

  /// Linux: files or folders dropped on the window wait for a device tap.
  Widget dropZone(ThemeData theme, Widget child) {
    if (!Platform.isLinux) return child;
    final c = theme.colorScheme;
    return DropTarget(
      onDragEntered: (_) => setState(() => dragging = true),
      onDragExited: (_) => setState(() => dragging = false),
      onDragDone: (d) {
        setState(() {
          dragging = false;
          if (d.files.isNotEmpty) pending = [for (final f in d.files) uft.Pick(path: f.path)];
        });
      },
      child: Stack(
        children: [
          child,
          Positioned.fill(
            child: IgnorePointer(
              child: AnimatedOpacity(
                opacity: dragging ? 1 : 0,
                duration: const Duration(milliseconds: 150),
                child: Container(
                  margin: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: c.primaryContainer.withValues(alpha: 0.92),
                    borderRadius: BorderRadius.circular(24),
                    border: Border.all(color: c.primary, width: 2),
                  ),
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Icon(Icons.upload_file, size: 48, color: c.onPrimaryContainer),
                      const SizedBox(height: 12),
                      Text(
                        'Drop to send',
                        style: theme.textTheme.titleLarge?.copyWith(color: c.onPrimaryContainer),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        'then tap a device',
                        style: theme.textTheme.bodyMedium?.copyWith(color: c.onPrimaryContainer),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class TransferCard extends StatelessWidget {
  const TransferCard(this.t, {super.key});
  final Transfer t;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final c = theme.colorScheme;
    final running = t.result == null;
    final frac = t.total > 0 ? (t.done / t.total).clamp(0.0, 1.0) : null;
    final (icon, tint) = t.failed
        ? (Icons.error_outline, c.error)
        : running
        ? (t.sending ? Icons.arrow_upward : Icons.arrow_downward, c.primary)
        : (Icons.check_circle, c.primary);
    final muted = theme.textTheme.bodySmall?.copyWith(
      color: c.onSurfaceVariant,
      fontFeatures: tabular,
    );
    return Card(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 8, 14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              height: 40,
              child: Row(
                children: [
                  AnimatedSwitcher(
                    duration: const Duration(milliseconds: 250),
                    transitionBuilder: (child, a) => ScaleTransition(scale: a, child: child),
                    child: Icon(icon, key: ValueKey(icon), color: tint, size: 20),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      t.title,
                      style: theme.textTheme.titleSmall,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  if (running && frac != null && t.note == null)
                    Padding(
                      padding: const EdgeInsets.only(left: 8, right: 8),
                      child: Text(
                        '${(frac * 100).toStringAsFixed(0)}%',
                        style: theme.textTheme.labelLarge?.copyWith(
                          color: c.primary,
                          fontFeatures: tabular,
                        ),
                      ),
                    ),
                  if (running && t.sendId != null)
                    TextButton(
                      onPressed: () => uft.cancel(id: t.sendId!),
                      child: const Text('Cancel'),
                    ),
                ],
              ),
            ),
            const SizedBox(height: 8),
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: running && frac == null
                  ? const LinearProgressIndicator()
                  : TweenAnimationBuilder<double>(
                      tween: Tween(end: running ? frac : 1),
                      duration: const Duration(milliseconds: 400),
                      curve: Curves.easeOutCubic,
                      builder: (context, v, _) =>
                          LinearProgressIndicator(value: v, color: t.failed ? c.error : null),
                    ),
            ),
            const SizedBox(height: 10),
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: t.note != null
                  ? Text(t.note!, style: muted)
                  : running
                  ? SizedBox(
                      width: double.infinity,
                      child: Wrap(
                        alignment: WrapAlignment.spaceBetween,
                        spacing: 12,
                        runSpacing: 2,
                        children: [
                          Text('${size(t.done)} of ${size(t.total)}', style: muted),
                          Text(
                            '${speed(t.speed)} · peak ${speed(t.peak)}'
                            '${t.eta == null ? '' : ' · ${duration(t.eta!)} left'}',
                            style: muted,
                          ),
                        ],
                      ),
                    )
                  : Text(t.result!, style: t.failed ? muted?.copyWith(color: c.error) : muted),
            ),
          ],
        ),
      ),
    );
  }
}
