import 'dart:async';
import 'dart:convert';
import 'dart:io';

typedef DirectGatewayResponse = FutureOr<Map<String, Object?>?> Function(
  Map<String, Object?> request,
  int connectionNumber,
);

final class DirectRuntimeGateway {
  DirectRuntimeGateway._(this._server, this._subscription, this._response);

  static Future<DirectRuntimeGateway> start({
    required DirectGatewayResponse response,
    int port = 0,
  }) async {
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, port);
    late final DirectRuntimeGateway gateway;
    final subscription = server.listen((request) async {
      final socket = await WebSocketTransformer.upgrade(request);
      gateway._sockets.add(socket);
      gateway._socketClosed.add(Completer<void>());
      final connectionNumber = gateway._sockets.length;
      socket.listen(
        (raw) async {
          final message = jsonDecode(raw as String) as Map<String, Object?>;
          gateway.requests.add(message);
          final payload = await gateway._response(message, connectionNumber);
          if (payload == null || socket.readyState != WebSocket.open) return;
          _reply(
            socket,
            jsonEncode(<String, Object?>{
              'id': message['id'],
              'ok': true,
              'payload': payload,
            }),
          );
        },
        onDone: () {
          final closed = gateway._socketClosed[connectionNumber - 1];
          if (!closed.isCompleted) closed.complete();
        },
      );
    });
    gateway = DirectRuntimeGateway._(server, subscription, response);
    return gateway;
  }

  final HttpServer _server;
  final StreamSubscription<HttpRequest> _subscription;
  final DirectGatewayResponse _response;
  final List<WebSocket> _sockets = <WebSocket>[];
  final List<Completer<void>> _socketClosed = <Completer<void>>[];
  final List<Map<String, Object?>> requests = <Map<String, Object?>>[];

  String get endpoint => 'ws://${_server.address.address}:${_server.port}';

  List<WebSocket> get sockets => List<WebSocket>.unmodifiable(_sockets);

  Future<void> closeSocket(int index) => _sockets[index].close();

  Future<void> waitForSocketClose(int index) => _socketClosed[index].future;

  void reply(
    int socketIndex,
    Map<String, Object?> request,
    Map<String, Object?> payload,
  ) {
    _reply(
      _sockets[socketIndex],
      jsonEncode(<String, Object?>{
        'id': request['id'],
        'ok': true,
        'payload': payload,
      }),
    );
  }

  Future<void> dispose() async {
    await _subscription.cancel();
    for (final socket in _sockets) {
      await socket.close();
    }
    await _server.close(force: true);
  }
}

void _reply(WebSocket socket, String message) {
  try {
    socket.add(message);
  } on StateError catch (error) {
    if (error.message != 'StreamSink is closed') rethrow;
  }
}
