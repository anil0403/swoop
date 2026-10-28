import 'dart:async';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:uft/src/rust/api/uft.dart' as uft;
import 'package:uft/src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(const App());
}

class App extends StatelessWidget {
  const App({super.key});

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'uft',
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
  String? result;
  bool failed = false;
}

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
  State<Home> createState() => _HomeState();
}

class _HomeState extends State<Home> {
  uft.Me? me;
  String? dir, error;
  List<uft.Peer> peers = [];
  final transfers = <Transfer>[];
  final incoming = <int, Transfer>{};
  bool scanning = false;
  Timer? rescan;

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
      final downloads = await getDownloadsDirectory() ?? await getApplicationDocumentsDirectory();
      final d = '${downloads.path}/uft';
      final m = await uft.init(configDir: config.path, name: Platform.localHostname);
      setState(() {
        me = m;
        dir = d;
      });
      uft.receive(dir: d).listen(onIncoming, onError: (Object e) => setState(() => error = '$e'));
      scan();
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
            setState(() {
              peers = latest;
              scanning = false;
            });
            rescan = Timer(const Duration(seconds: 4), scan);
          },
        );
  }

  void onIncoming(uft.Incoming e) {
    switch (e) {
      case uft.Incoming_Offer():
        offer(e);
      case uft.Incoming_Started():
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
        final t = incoming.remove(e.session);
        if (t != null) {
          setState(() {
            t.done = t.total;
            t.result = 'Received ${size(e.bytes)} in ${duration(e.secs)} · saved to $dir';
          });
        }
      case uft.Incoming_Failed():
        final t = incoming.remove(e.session);
        if (t != null) {
          setState(
            () => t
              ..failed = true
              ..result = e.error,
          );
        }
    }
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
    final folder = await showModalBottomSheet<bool>(
      context: context,
      builder: (context) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(title: Text('Send to ${p.name} · code ${p.code}')),
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
    if (folder == null) return;
    final paths = folder
        ? [?await getDirectoryPath()]
        : (await openFiles()).map((f) => f.path).toList();
    if (paths.isNotEmpty) send(p, paths);
  }

  void send(uft.Peer p, List<String> paths) {
    final t = Transfer(
      'To ${p.name} · ${paths.length == 1 ? paths.first.split('/').last : '${paths.length} items'}',
    );
    setState(() => transfers.insert(0, t));
    uft
        .sendFiles(peer: p, paths: paths)
        .listen(
          (e) => setState(() {
            switch (e) {
              case uft.Outgoing_Progress():
                t
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
                      '${e.resumed > 0 ? ' · ${size(e.resumed)} already there' : ''}';
              case uft.Outgoing_Failed():
                t
                  ..failed = true
                  ..result = e.error;
            }
          }),
          onError: (Object e) => setState(
            () => t
              ..failed = true
              ..result = '$e',
          ),
        );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text(me == null ? 'uft' : 'uft · ${me!.name}'),
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
          if (peers.isEmpty)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 16),
              child: Text('Looking for devices on this network… Open uft on the other device.'),
            ),
          for (final p in peers)
            Card(
              child: ListTile(
                leading: Icon(p.os == 'android' ? Icons.phone_android : Icons.computer),
                title: Text(p.name),
                subtitle: Text('${p.addr} · code ${p.code}'),
                trailing: const Icon(Icons.send),
                onTap: () => pick(p),
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
              running
                  ? '${frac == null ? '' : '${(frac * 100).toStringAsFixed(1)}%  '}'
                        '${size(t.done)} / ${size(t.total)}  ${speed(t.speed)}  peak ${speed(t.peak)}'
                        '${t.eta == null ? '' : '  ETA ${duration(t.eta!)}'}'
                  : t.result!,
            ),
          ],
        ),
      ),
    );
  }
}
