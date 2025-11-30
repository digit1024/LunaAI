import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/application/app_state.dart';

void main() {
  test('app state starts in setup pane', () {
    final state = AppState.initial();
    expect(state.connection, ConnectionStatus.connecting);
    expect(state.pane, ActivePane.setup);
    expect(state.conversations, isEmpty);
  });
}


