import 'dart:async';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:uft/src/rust/api/uft.dart' as uft;
import 'package:uft/src/rust/frb_generated.dart';

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
    theme: ThemeData(colorSchemeSeed: Colors.teal),
    darkTheme: ThemeData(colorSchemeSeed: Colors.teal, brightness: Brightness.dark),
    home: const Home(),
  );
}

/// One transfer card, sending or receiving.
class Transfer {
  Transfer(this.title, {this.total = 0});
  final String title;
  int done = 0, total;
  double speed = 0, peak = 0;
  double? eta;

  /// What it's waiting for, before bytes move.
  String? note;
  String? result;
  bool failed = false;

  /// A running send's id, for `uft.cancel`.
  int? sendId;
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
  bool scanning = false, busy = false;
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
      uft.receive(dir: d).listen(onIncoming, onError: (Object e) => setState(() => error = '$e'));
      scan();
      startBle();
    } catch (e) {
      setState(() => error = '$e');
    }
  }

  /// Scans for a few seconds, then again shortly after; devices that stop answering drop off.
  void scan() {
    if (!mounted) return;
    setState(() => scanning = true);
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
          onError: (Object e) => setState(() => error = '$e'),
          onDone: () {
            if (!mounted) return;
            final stale = DateTime.now().subtract(const Duration(seconds: 15));
            setState(() {
              peers = latest;
              scanning = false;
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
            t.result = 'Received ${size(e.bytes)} in ${duration(e.secs)} · saved to $dir';
          });
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
        title: Text('${o.peerName} wants to send you ${o.files} file${o.files == 1 ? '' : 's'}'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(size(o.total)),
            const SizedBox(height: 16),
            const Text('Check that the other device shows the same code:'),
            const SizedBox(height: 8),
            Text(o.code, style: Theme.of(context).textTheme.headlineMedium),
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
    final picks = await choose('Send to ${p.name} · code ${p.code}');
    if (picks.isNotEmpty) send(p, picks);
  }

  /// Files or a folder, from the platform's picker.
  Future<List<uft.Pick>> choose(String title) async {
    final folder = await showModalBottomSheet<bool>(
      context: context,
      builder: (context) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(title: Text(title)),
            ListTile(
              leading: const Icon(Icons.description),
              title: const Text('Files'),
              onTap: () => Navigator.pop(context, false),
            ),
            ListTile(
              leading: const Icon(Icons.folder),
              title: const Text('Folder'),
              onTap: () => Navigator.pop(context, true),
            ),
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
    if (now != busy && Platform.isAndroid) android.invokeMethod('busy', now);
    busy = now;
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
              ..result = '$e',
          ),
        );
  }

  /// To a device seen only over Bluetooth: host a hotspot, invite it over Bluetooth, wait until it's on the hotspot,
  /// send, take the hotspot down.
  Future<void> sendDirect(direct.BlePeer b) async {
    final picks = await choose('Send to ${b.name} · direct Wi-Fi link');
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
          ..result = '$e',
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
      setState(() => error = "Couldn't join ${inv.name}'s hotspot: $e");
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
    final target = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Connect by address'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('On the other device, type one of these:'),
            for (final l in uft.myLinks()) SelectableText(l, style: const TextStyle(fontSize: 12)),
            const SizedBox(height: 16),
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
      setState(() => error = '$e');
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

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text(me == null ? 'Swoop' : 'Swoop · ${me!.name}'),
        actions: [
          IconButton(
            icon: const Icon(Icons.add_link),
            tooltip: 'Connect by address',
            onPressed: me == null ? null : connectByAddress,
          ),
        ],
        bottom: scanning
            ? const PreferredSize(
                preferredSize: Size.fromHeight(2),
                child: LinearProgressIndicator(minHeight: 2),
              )
            : null,
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          if (error != null)
            Card(
              color: theme.colorScheme.errorContainer,
              child: ListTile(leading: const Icon(Icons.error), title: Text(error!)),
            ),
          if (dir != null)
            Text('Visible to nearby devices · saving to $dir', style: theme.textTheme.bodySmall),
          const SizedBox(height: 16),
          Text('Nearby devices', style: theme.textTheme.titleMedium),
          if (peers.isEmpty && manual.isEmpty)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 16),
              child: Text('Looking for devices on this network… Open Swoop on the other device.'),
            ),
          for (final p in [
            ...peers,
            ...manual.where((m) => !peers.any((p) => p.fingerprint == m.fingerprint)),
          ])
            Card(
              child: ListTile(
                leading: Icon(p.os == 'android' ? Icons.phone_android : Icons.computer),
                title: Text(p.name),
                subtitle: Text(
                  '${p.addr} · code ${p.code}'
                  '${p.link == null ? '' : '\n${p.link}'}'
                  '${p.expected == null ? '' : ' · up to ${speed(p.expected!)}'}',
                ),
                trailing: const Icon(Icons.send),
                onTap: () => pick(p),
              ),
            ),
          for (final b in ble.values.where(
            (b) => ![...peers, ...manual].any((p) => p.fingerprint.startsWith(b.fp)),
          ))
            Card(
              child: ListTile(
                leading: const Icon(Icons.bluetooth),
                title: Text(b.name.isEmpty ? 'Swoop device' : b.name),
                subtitle: const Text('Not on this network · sends over a direct Wi-Fi link'),
                trailing: const Icon(Icons.send),
                onTap: () => sendDirect(b),
              ),
            ),
          if (bleError != null)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Text(
                'Bluetooth: $bleError. Devices on other networks only show up with Bluetooth on.',
                style: theme.textTheme.bodySmall,
              ),
            ),
          if (transfers.isNotEmpty) ...[
            const SizedBox(height: 16),
            Text('Transfers', style: theme.textTheme.titleMedium),
          ],
          for (final t in transfers) TransferCard(t),
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
    final running = t.result == null;
    final frac = t.total > 0 ? t.done / t.total : null;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(t.title, style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            LinearProgressIndicator(
              value: running ? frac : 1,
              color: t.failed ? Theme.of(context).colorScheme.error : null,
            ),
            const SizedBox(height: 8),
            Text(
              t.note != null
                  ? t.note!
                  : running
                  ? '${frac == null ? '' : '${(frac * 100).toStringAsFixed(1)}%  '}'
                        '${size(t.done)} / ${size(t.total)}  ${speed(t.speed)}  peak ${speed(t.peak)}'
                        '${t.eta == null ? '' : '  ETA ${duration(t.eta!)}'}'
                  : t.result!,
            ),
            if (running && t.sendId != null)
              Align(
                alignment: Alignment.centerRight,
                child: TextButton(
                  onPressed: () => uft.cancel(id: t.sendId!),
                  child: const Text('Cancel'),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
