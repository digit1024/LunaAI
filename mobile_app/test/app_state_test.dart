import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/application/app_state.dart';

void main() {
  test('app state starts in connecting pane', () {
    final state = AppState.initial();
    expect(state.connection, ConnectionStatus.connecting);
    expect(state.pane, ActivePane.connecting);
    expect(state.conversations, isEmpty);
  });
}


