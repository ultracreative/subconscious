#!/usr/bin/env bash

exec python3 - "$@" << 'EOF'
import sys
import os
import time
import sqlite3

room_id = sys.argv[1] if len(sys.argv) > 1 else "room-feec8c1e-d1d7-465a-9e01-b9236a43329f"
db_path = os.path.expanduser("~/.local/share/cortexkit/uc-discussions/store.db")

if not os.path.exists(db_path):
    print(f"Error: Database not found at {db_path}")
    sys.exit(1)

def get_conn():
    conn = sqlite3.connect(db_path, timeout=5.0)
    conn.row_factory = sqlite3.Row
    return conn

def print_header(topic, status):
    if sys.stdout.isatty():
        os.system("clear")
    print("\033[1;36m" + "=" * 80 + "\033[0m")
    print(f"\033[1;32m  DELIBERATION ROOM MONITOR: {room_id}\033[0m")
    print(f"\033[1;33m  Topic : \033[0m{topic}")
    print(f"\033[1;33m  Status: \033[0m{status}")
    print("\033[1;36m" + "=" * 80 + "\033[0m\n")

try:
    conn = get_conn()
    cur = conn.cursor()
    cur.execute("SELECT topic, status FROM rooms WHERE room_id = ?", (room_id,))
    row = cur.fetchone()
    if not row:
        print(f"Room {room_id} not found.")
        sys.exit(1)
    topic = row["topic"]
    current_status = row["status"]
    print_header(topic, current_status)
    conn.close()
except Exception as e:
    print(f"Error reading initial room info: {e}")
    sys.exit(1)

last_seq = 0

while True:
    try:
        conn = get_conn()
        cur = conn.cursor()
        
        cur.execute("SELECT status FROM rooms WHERE room_id = ?", (room_id,))
        row = cur.fetchone()
        if row and row["status"] != current_status:
            print(f"\033[1;31m>>> Room status changed: {current_status} -> {row['status']} <<<\033[0m\n")
            current_status = row["status"]

        cur.execute(
            "SELECT seq, author, post_type, content, created_at FROM room_posts WHERE room_id = ? AND seq > ? ORDER BY seq ASC",
            (room_id, last_seq),
        )
        posts = cur.fetchall()
        for p in posts:
            seq = p["seq"]
            author = p["author"]
            ptype = p["post_type"]
            created = p["created_at"]
            content = p["content"]
            
            print(f"\033[1;34m[Seq {seq}]\033[0m \033[1;35m{author}\033[0m (\033[1;32m{ptype}\033[0m) \033[2m[{created}]\033[0m")
            print(content)
            print()
            last_seq = seq

        conn.close()
    except Exception as e:
        pass

    time.sleep(1.5)
EOF
