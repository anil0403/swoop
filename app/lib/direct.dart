import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:uft/src/rust/api/direct.dart' as linux;

export 'package:uft/src/rust/api/direct.dart' show Wifi;

/// The direct link for devices with no shared network (PLAN.md, Phase 5): Bluetooth LE to find and invite, a hotspot
/// to transfer over. Android: Direct.kt; Linux: app/rust/src/api/direct.rs.
const _channel = MethodChannel('uft');
const _events = EventChannel('uft/ble');

/// A uft device seen over Bluetooth; `fp` is the first 16 hex chars of its fingerprint.
class BlePeer {
  BlePeer(this.id, this.name, this.fp);
  final String id, name, fp;
  DateTime seen = DateTime.now();
}

/// What a sender writes to a receiver over Bluetooth: join this network, then expect files from `fp`.
class Invite {
  const Invite(this.wifi, this.fp, this.name);
  final linux.Wifi wifi;
  final String fp, name;

  String toJson() => jsonEncode({
    'v': 1,
    'ssid': wifi.ssid,
    'pass': wifi.pass,
    'sec': wifi.sec,
    'fp': fp,
    // Keeps the invite within one Bluetooth write.
    'name': name.length > 32 ? name.substring(0, 32) : name,
  });

  static Invite? parse(String json) {
    try {
      final m = jsonDecode(json) as Map<String, dynamic>;
      if (m['v'] != 1) return null;
      return Invite(
        linux.Wifi(ssid: m['ssid'] as String, pass: m['pass'] as String, sec: m['sec'] as String),
        m['fp'] as String,
        m['name'] as String,
      );
    } catch (_) {
      return null;
    }
  }
}

StreamSubscription<Object?>? _android;

/// Advertises this device and reports others and incoming invites, until the app exits. Errors (Bluetooth off,
/// no permission) go to `failed`; calling it again retries.
Future<void> bleStart(
  String fp,
  String name, {
  required void Function(BlePeer) seen,
  required void Function(String json) invite,
  required void Function(String error) failed,
}) async {
  if (Platform.isAndroid) {
    _android ??= _events.receiveBroadcastStream().listen((e) {
      final m = (e as Map).cast<String, String>();
      m['type'] == 'seen' ? seen(BlePeer(m['id']!, m['name']!, m['fp']!)) : invite(m['json']!);
    });
    try {
      await _channel.invokeMethod('bleStart', {'fp': fp, 'name': name});
    } on PlatformException catch (e) {
      failed(e.message ?? e.code);
    }
  } else {
    linux
        .bleStart(fp: fp, name: name)
        .listen(
          (e) => switch (e) {
            linux.BleEvent_Seen() => seen(BlePeer(e.id, e.name, e.fp)),
            linux.BleEvent_Invite() => invite(e.json),
          },
          onError: (Object e) => failed('$e'),
        );
  }
}

Future<void> bleInvite(String id, String json) => Platform.isAndroid
    ? _call('bleInvite', {'id': id, 'json': json})
    : linux.bleInvite(id: id, json: json);

/// Starts a hotspot on this device and returns how to join it.
Future<linux.Wifi> host() async {
  if (!Platform.isAndroid) return linux.hostWifi();
  final m = (await _call<Map<Object?, Object?>>('host'))!.cast<String, String>();
  return linux.Wifi(ssid: m['ssid']!, pass: m['pass']!, sec: m['sec']!);
}

Future<void> stopHost() => Platform.isAndroid ? _call('stopHost') : linux.stopHost();

/// Joins `wifi`; returns once this device is on it (Android 10+ asks the user first).
Future<void> join(linux.Wifi wifi) => Platform.isAndroid
    ? _call('join', {'ssid': wifi.ssid, 'pass': wifi.pass, 'sec': wifi.sec})
    : linux.joinWifi(wifi: wifi);

Future<void> leave() => Platform.isAndroid ? _call('leave') : linux.leaveWifi();

/// Platform errors as plain messages, like the Rust side's.
Future<T?> _call<T>(String method, [Object? args]) async {
  try {
    return await _channel.invokeMethod<T>(method, args);
  } on PlatformException catch (e) {
    throw e.message ?? e.code;
  }
}
