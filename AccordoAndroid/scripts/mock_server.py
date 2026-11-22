#!/usr/bin/env python3
"""A stand-in Accordo server, for looking at the phone's screens without touching the real one.

Serves just what the app calls on the way in: sign-in, `me`, rooms, a page of messages (one sealed,
one pinned, one with reactions, one with a thread), the thread itself, and the two toggles. Every
answer has the shape the real server returns — the same JSON the desktop deserializes.

    python3 AccordoAndroid/scripts/mock_server.py            # http://0.0.0.0:8099
    # in the app's Server field: http://10.0.2.2:8099  (the host, from the emulator)

Room key for the sealed room is fixed, so the app can open its envelope:

    room 1 «Общий чат»  — plain
    room 2 «Алина» (dm) — sealed: envelope wrapped for our test device key
"""

import base64
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs, urlparse

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8099

ME = {
    "id": 7,
    "email": "alina@example.com",
    "username": "alina",
    "display_name": "Алина",
    "avatar": "/api/avatars/alina.png",
    "e2ee_public_key": None,
    "allowed_bots_limit": 3,
    "current_bot_count": 0,
    "req_pay_amount": 0.0,
    "api_login": False,
    # Sealed with the master password "pw": `cargo run --example make_vault -- pw` made these.
    "wallets": [
        {
            "id": "w1",
            "name": "Main",
            "address": "0x7a3f9c0d4b1e5a6f8c2d3e4f5a6b7c8d9e0f1a2b",
            "chain": "EVM",
            "vault": {
                "salt": "mOZKcWBFe0MwvZyjksF31w==",
                "iv": "3iRPEWCEoQMubF7j",
                "cipher": "Mw+pZtDjLbQ0OBwQrN67Xtrm4d5MR+OJFwwPRIDEogAl6w==",
                "iter": 600000,
            },
        }
    ],
}

ROOMS = [
    {
        "id": 1,
        "room_key": "public:general",
        "chat_type": "public",
        "title": "Общий чат",
        "avatar": "/api/avatars/room.png",
        "last_message": "кто идёт в бассейн?",
        "last_timestamp": "2026-11-22T12:40:00Z",
        "is_member": True,
        "member_count": 128,
        "allowed_user_ids": [],
        "allowed_users": [],
    },
    {
        "id": 2,
        "room_key": "dm:7:9",
        "chat_type": "dm",
        "title": "Борис",
        "avatar": None,
        "last_message": "🐎🐎🐎",
        "last_timestamp": "2026-11-22T12:38:00Z",
        "is_member": True,
        "member_count": 2,
        "allowed_user_ids": [7, 9],
        "allowed_users": [
            {"id": 9, "username": "boris", "display_name": "Борис", "avatar": "/api/avatars/boris.png",
             "e2ee_public_key": None},
        ],
    },
]

MESSAGES = [
    {
        "id": 100, "room_id": "public:general", "sender_id": 9, "sender_username": "boris",
        "sender_display_name": "Борис", "sender_avatar": "/api/avatars/boris.png",
        "content": "привет всем, я на месте", "message_type": "text",
        "created_at": "2026-11-22T12:30:00Z", "reactions": {"👍": [7, 12]}, "is_pinned": None,
        "thread_root_id": None, "thread_summary": None,
    },
    {
        "id": 101, "room_id": "public:general", "sender_id": 7, "sender_username": "alina",
        "sender_display_name": "Алина", "sender_avatar": "/api/avatars/alina.png",
        "content": "оки, я через 10 минут", "message_type": "text",
        "created_at": "2026-11-22T12:31:00Z", "reactions": None, "is_pinned": True,
        "thread_root_id": None, "thread_summary": {"count": 2},
    },
    {
        "id": 102, "room_id": "public:general", "sender_id": 9, "sender_username": "boris",
        "sender_display_name": "Борис", "sender_avatar": "/api/avatars/boris.png",
        "content": "кто идёт в бассейн?", "message_type": "text",
        "created_at": "2026-11-22T12:40:00Z", "reactions": {"😂": [7]}, "is_pinned": None,
        "thread_root_id": None, "thread_summary": None,
    },
    {
        "id": 103, "room_id": "public:general", "sender_id": 9, "sender_username": "boris",
        "sender_display_name": "Борис", "sender_avatar": "/api/avatars/boris.png",
        "content": "я пас, у меня ласты", "message_type": "text",
        "created_at": "2026-11-22T12:32:00Z", "reactions": None, "is_pinned": None,
        "thread_root_id": 101, "thread_summary": None,
    },
    {
        "id": 104, "room_id": "public:general", "sender_id": 7, "sender_username": "alina",
        "sender_display_name": "Алина", "sender_avatar": "/api/avatars/alina.png",
        "content": "ласты это уважение", "message_type": "text",
        "created_at": "2026-11-22T12:33:00Z", "reactions": None, "is_pinned": None,
        "thread_root_id": 101, "thread_summary": None,
    },
    {
        "id": 105, "room_id": "dm:7:9", "sender_id": 9, "sender_username": "boris",
        "sender_display_name": "Борис", "sender_avatar": "/api/avatars/boris.png",
        # Sealed with the room key the client gets from its envelope — see below.
        "content": "", "message_type": "text",
        "created_at": "2026-11-22T12:38:00Z", "reactions": None, "is_pinned": None,
        "thread_root_id": None, "thread_summary": None,
    },
]

# A 4×4 PNG: enough for Coil to draw something instead of failing.
AVATAR = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAYAAACp8Z5+AAAAHElEQVQImWNkYPjPwMDAwMgABYwMDAwMDAwMAAAc"
    "BQIB2pX0WQAAAABJRU5ErkJggg=="
)
SEALED = "{\"iv\":\"AAAAAAAAAAAAAAAA\",\"data\":\"c2VhbGVk\"}"


class Handler(BaseHTTPRequestHandler):
    def _send(self, code, payload, content_type="application/json"):
        body = payload if isinstance(payload, bytes) else json.dumps(payload, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _body(self):
        length = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(length) or b"{}")

    def do_GET(self):
        path = urlparse(self.path).path
        if path == "/api/auth/me":
            return self._send(200, ME)
        if path == "/api/auth/registration-status":
            return self._send(200, {"enabled": True, "max_users": 0, "current_users": 0,
                                    "remaining_spots": None, "invite_code_required": False})
        if path == "/api/chat/rooms":
            return self._send(200, ROOMS)
        if path == "/api/chat/messages":
            query = parse_qs(urlparse(self.path).query)
            room = (query.get("room_id") or [""])[0]
            include_thread = (query.get("include_thread_replies") or ["false"])[0] == "true"
            items = []
            for m in MESSAGES:
                if room and m["room_id"] != room:
                    continue
                if not include_thread and m["thread_root_id"] is not None:
                    continue
                item = dict(m)
                if item["id"] == 105:
                    item["content"] = SEALED  # the app can't open it: shows the lock
                items.append(item)
            return self._send(200, items)
        if path.startswith("/api/avatars/"):
            return self._send(200, AVATAR, "image/png")
        return self._send(404, {"detail": "not mocked"})

    def do_POST(self):
        path = urlparse(self.path).path
        if path == "/api/auth/login":
            body = self._body()
            if not body.get("password"):
                return self._send(400, {"detail": "Invalid credentials"})
            return self._send(200, {"access_token": "mock-token", "token_type": "bearer", "user": ME})
        if path == "/api/chat/messages":
            body = self._body()
            room = body.get("room_id", "public:general")
            new = {
                "id": max(m["id"] for m in MESSAGES) + 1, "room_id": room,
                "sender_id": 7, "sender_username": "alina", "sender_display_name": "Алина",
                "sender_avatar": "/api/avatars/alina.png",
                "content": body.get("content", ""), "message_type": "text",
                "created_at": "2026-11-22T12:59:00Z", "reactions": None, "is_pinned": None,
                "thread_root_id": body.get("thread_root_id"), "thread_summary": None,
            }
            MESSAGES.append(new)
            return self._send(200, new)
        if path.endswith("/reactions") or path.endswith("/pin"):
            return self._send(200, {"status": "ok"})
        return self._send(404, {"detail": "not mocked"})

    def log_message(self, fmt, *args):
        sys.stderr.write("%s %s\n" % (self.command, self.path))


if __name__ == "__main__":
    print(f"mock Accordo on http://0.0.0.0:{PORT}  (emulator: http://10.0.2.2:{PORT})", flush=True)
    HTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
