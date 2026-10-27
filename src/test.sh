#!/bin/bash
# Запуск четырёх клиентов в tmux (сервер — в отдельном терминале).

SESSION="chess_clients"

tmux kill-session -t "$SESSION" 2>/dev/null

# Сетка 2×2: сначала одно окно, потом split.
tmux new-session -d -s "$SESSION" -c "$HOME/Rust"
tmux split-window -h -c "$HOME/Rust"          # верх: слева | справа
tmux select-pane -t 0
tmux split-window -v -c "$HOME/Rust"          # левая колонка: верх/низ
tmux select-pane -t 2
tmux split-window -v -c "$HOME/Rust"          # правая колонка: верх/низ

# Клиент 1 (верх-лево).
tmux send-keys -t "$SESSION:0.0" "cargo run -p chess_client_ui" C-m
# Клиент 2 (верх-право).
tmux send-keys -t "$SESSION:0.2" "cargo run -p chess_client_ui" C-m
# Клиент 3 (низ-лево).
tmux send-keys -t "$SESSION:0.1" "cargo run -p chess_client_ui" C-m
# Клиент 4 (низ-право).
tmux send-keys -t "$SESSION:0.3" "cargo run -p chess_client_ui" C-m

echo "tmux-сессия '$SESSION' запущена в фоне (4 клиента, сетка 2×2)."
echo "Подключиться: tmux attach -t $SESSION"
echo "Закрыть:       tmux kill-session -t $SESSION"
