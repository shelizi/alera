import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter/foundation.dart';

/// A view onto a tab that belongs to the main Alera instance.
/// The record itself stays in the workbench database and is never closed.
class DetachedTabDescriptor {
  const DetachedTabDescriptor({required this.workspace, required this.tab});

  final Workspace workspace;
  final WorkspaceTabRecord tab;

  Map<String, Object?> toJson() => {
    'workspace': workspace.toMap(),
    'tab': tab.toMap(),
  };

  factory DetachedTabDescriptor.fromJson(Map<String, dynamic> json) {
    return DetachedTabDescriptor(
      workspace: Workspace.fromJson(
        Map<String, Object?>.from(json['workspace'] as Map),
      ),
      tab: WorkspaceTabRecord.fromJson(
        Map<String, Object?>.from(json['tab'] as Map),
      ),
    );
  }
}

/// Only Windows detached child invocations should bypass the native single
/// instance gate. Never accept a partial or malformed request as a child.
class DetachedTabLaunchArguments {
  const DetachedTabLaunchArguments(this.port, this.token);

  static const flag = '--alera-detached-tab';
  final int port;
  final String token;

  static DetachedTabLaunchArguments? parse(List<String> args) {
    if (args.length != 3 || args[0] != flag) return null;
    final port = int.tryParse(args[1]);
    if (port == null || port < 1 || port > 65535) return null;
    if (!RegExp(r'^[a-f0-9]{40}$').hasMatch(args[2])) return null;
    return DetachedTabLaunchArguments(port, args[2]);
  }
}

typedef DetachedTabProcessStarter = Future<void> Function(
  int port,
  String token,
);

/// Tests may supply a fake launcher; production opens the same exe in a new
/// native Windows process. Each child uses its own single-instance identity.
Future<void> startDetachedTabProcess(int port, String token) async {
  if (!Platform.isWindows) {
    throw UnsupportedError('Detaching tabs is currently supported on Windows.');
  }
  await Process.start(Platform.resolvedExecutable, <String>[
    DetachedTabLaunchArguments.flag,
    '$port',
    token,
  ], mode: ProcessStartMode.detached);
}

/// The child sees only a loopback one-time authenticated launch channel.
/// A failed child never causes the tab to disappear from the parent.
class DetachedTabWindowService extends ChangeNotifier {
  DetachedTabWindowService({
    DetachedTabProcessStarter starter = startDetachedTabProcess,
    Duration handshakeTimeout = const Duration(seconds: 45),
  }) : _starter = starter,
       _timeout = handshakeTimeout;

  final DetachedTabProcessStarter _starter;
  final Duration _timeout;

  /// Navigation is routed to the primary workbench. Child windows never
  /// create duplicate workspace tab records.
  Future<void> Function(DetachedTabDescriptor, String relativePath)? onOpenFile;

  /// Verify the pointer is over the primary native HWND, not merely within
  /// its bounds (which could include the detached window).
  Future<bool> Function(DetachedTabDescriptor)? canDockAtPointer;
  final Map<String, Socket> _active = {};
  final Set<String> _pending = {};
  bool _disposed = false;

  Set<String> get detachedTabIds => Set.unmodifiable(_active.keys);
  bool isDetached(String tabId) => _active.containsKey(tabId);
  bool isLaunching(String tabId) => _pending.contains(tabId);

  Future<bool> detach(DetachedTabDescriptor descriptor) async {
    final tabId = descriptor.tab.id;
    if (_disposed || _active.containsKey(tabId) || !_pending.add(tabId)) {
      return false;
    }
    ServerSocket? server;
    Socket? socket;
    StreamSubscription<Socket>? accepting;
    try {
      server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
      // Subscribe before launching: a failing child must not leave a
      // dangling Stream.first future that throws after the server closes.
      final connection = Completer<Socket>();
      accepting = server.listen(
        (client) {
          if (!connection.isCompleted) {
            connection.complete(client);
          } else {
            client.destroy();
          }
        },
        onError: (Object error) {
          if (!connection.isCompleted) connection.completeError(error);
        },
      );
      final token = List<int>.generate(
        20,
        (_) => Random.secure().nextInt(256),
      ).map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();
      await _starter(server.port, token);
      socket = await connection.future.timeout(_timeout);
      final ready = Completer<void>();
      var authenticated = false;
      var active = false;
      final connected = socket;
      connected
          .cast<List<int>>()
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .listen(
            (message) {
              if (!authenticated) {
                if (message != 'HELLO ' + token) {
                  if (!ready.isCompleted) {
                    ready.completeError(
                      StateError('Detached window authentication failed'),
                    );
                  }
                  connected.destroy();
                  return;
                }
                authenticated = true;
                connected.writeln(jsonEncode(descriptor.toJson()));
              } else if (!active && message == 'READY') {
                active = true;
                if (!_disposed) {
                  _active[tabId] = connected;
                  notifyListeners();
                }
                if (!ready.isCompleted) ready.complete();
              } else if (message == 'DOCK') {
                connected.destroy();
              } else if (active && message == 'DOCK_QUERY') {
                unawaited(() async {
                  var allowed = false;
                  try {
                    final checker = canDockAtPointer;
                    allowed = checker != null && await checker(descriptor);
                  } catch (_) {
                    // A failing platform hit test must never dock a tab.
                  }
                  try {
                    if (!_disposed && identical(_active[tabId], connected)) {
                      connected.writeln(
                        allowed ? 'DOCK_ALLOWED' : 'DOCK_DENIED',
                      );
                    }
                  } catch (_) {}
                }());
              } else if (active && message.startsWith('OPEN_FILE ')) {
                try {
                  final path = utf8.decode(
                    base64Url.decode(message.substring(10)),
                  );
                  if (path.isNotEmpty &&
                      !path.startsWith('/') &&
                      !path.startsWith('\\') &&
                      !path.contains('..') &&
                      !path.contains('\\') &&
                      !path.contains(':') &&
                      !path.contains('\n') &&
                      path.length <= 4096) {
                    final handler = onOpenFile;
                    if (handler != null) {
                      unawaited(
                        handler(descriptor, path).catchError((Object _) {}),
                      );
                    }
                  }
                } catch (_) {
                  // Ignore malformed navigation requests.
                }
              }
            },
            onError: (Object error) {
              if (!ready.isCompleted) ready.completeError(error);
              _finish(tabId, connected);
            },
            onDone: () {
              if (!ready.isCompleted) {
                ready.completeError(
                  StateError('Detached window closed before ready'),
                );
              }
              _finish(tabId, connected);
            },
          );
      await ready.future.timeout(_timeout);
      return !_disposed && _active[tabId] == connected;
    } finally {
      _pending.remove(tabId);
      await accepting?.cancel();
      await server?.close();
      if (!_active.containsKey(tabId)) socket?.destroy();
    }
  }

  void _finish(String tabId, Socket socket) {
    if (identical(_active[tabId], socket)) {
      _active.remove(tabId);
      if (!_disposed) notifyListeners();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    // Closing a socket can synchronously invoke its onDone listener, which
    // removes the entry from _active. Iterate over a stable snapshot so
    // closing the primary Alera process never throws a ConcurrentModification.
    for (final socket in _active.values.toList(growable: false)) {
      socket.destroy();
    }
    _active.clear();
    super.dispose();
  }
}

/// The singleton outlives workspace switches; a hidden tab must not
/// reappear while its window is still open merely because its widget unmounted.
final detachedTabWindows = DetachedTabWindowService();

/// Child-side protocol: initial record, ready acknowledgement and docking.
/// Keep the socket open for the lifetime of the detached native window.
class DetachedTabWindowClient {
  DetachedTabWindowClient._(
    this.descriptor,
    this._socket,
    this._subscription,
    this._responses,
  );

  final DetachedTabDescriptor descriptor;
  final Socket _socket;
  final StreamSubscription<String> _subscription;
  final StreamController<String> _responses;
  bool _docking = false;
  bool _checkingDock = false;

  static Future<DetachedTabWindowClient> connect(
    DetachedTabLaunchArguments args, {
    Duration timeout = const Duration(seconds: 12),
  }) async {
    final socket = await Socket.connect(
      InternetAddress.loopbackIPv4,
      args.port,
      timeout: timeout,
    );
    final first = Completer<DetachedTabDescriptor>();
    final responses = StreamController<String>.broadcast();
    var gotDescriptor = false;
    final subscription = socket
        .cast<List<int>>()
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen(
          (line) {
            if (gotDescriptor) {
              if (!responses.isClosed) responses.add(line);
              return;
            }
            gotDescriptor = true;
            try {
              final payload = jsonDecode(line) as Map<String, dynamic>;
              first.complete(DetachedTabDescriptor.fromJson(payload));
            } catch (error) {
              first.completeError(error);
            }
          },
          onError: (Object error) {
            if (!first.isCompleted) first.completeError(error);
          },
          onDone: () {
            if (!first.isCompleted) {
              first.completeError(StateError('Parent instance disconnected'));
            }
            unawaited(responses.close());
          },
        );
    socket.writeln('HELLO ' + args.token);
    try {
      final descriptor = await first.future.timeout(timeout);
      return DetachedTabWindowClient._(
        descriptor,
        socket,
        subscription,
        responses,
      );
    } catch (_) {
      await subscription.cancel();
      await responses.close();
      socket.destroy();
      rethrow;
    }
  }

  void ready() => _socket.writeln('READY');

  /// Request opening a linked file in the primary workbench model.
  void requestOpenFile(String relativePath) {
    if (_docking) return;
    _socket.writeln('OPEN_FILE ' + base64Url.encode(utf8.encode(relativePath)));
  }

  /// Only native hit testing in the primary process may authorize a dock.
  /// In particular, a child window overlapping the primary window is not
  /// sufficient; its HWND must not be under the cursor.
  Future<bool> canDockAtPointer() async {
    if (_docking || _checkingDock || _responses.isClosed) return false;
    _checkingDock = true;
    try {
      final response = _responses.stream
          .firstWhere((line) => line == 'DOCK_ALLOWED' || line == 'DOCK_DENIED')
          .timeout(const Duration(seconds: 3));
      _socket.writeln('DOCK_QUERY');
      return await response == 'DOCK_ALLOWED';
    } catch (_) {
      return false;
    } finally {
      _checkingDock = false;
    }
  }

  Future<void> dock() async {
    if (_docking) return;
    _docking = true;
    try {
      _socket.writeln('DOCK');
      await _socket.flush();
    } catch (_) {
      // The primary window may already have exited. The detached renderer
      // should still be closable once any dirty editor content is saved.
    } finally {
      await _subscription.cancel();
      await _responses.close();
      _socket.destroy();
    }
  }
}
