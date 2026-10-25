"""Loopback-only BEP 3/9/10 fixture peer. No trackers, DHT, or public swarm.

This is benchmark infrastructure, not a general torrent implementation.
"""
import hashlib
import os
import socketserver
import struct
import threading


def encode(value):
    if isinstance(value, int):
        return b"i" + str(value).encode() + b"e"
    if isinstance(value, str):
        value = value.encode()
    if isinstance(value, bytes):
        return str(len(value)).encode() + b":" + value
    if isinstance(value, dict):
        return b"d" + b"".join(encode(key) + encode(value[key]) for key in sorted(value)) + b"e"
    raise TypeError("Unsupported fixture bencode value")


def decode(data, offset=0):
    kind = data[offset:offset + 1]
    if kind == b"i":
        end = data.index(b"e", offset)
        return int(data[offset + 1:end]), end + 1
    if kind == b"d":
        result = {}
        offset += 1
        while data[offset:offset + 1] != b"e":
            key, offset = decode(data, offset)
            result[key], offset = decode(data, offset)
        return result, offset + 1
    colon = data.index(b":", offset)
    end = colon + 1 + int(data[offset:colon])
    return data[colon + 1:end], end


class Seed:
    def __init__(self, fixture):
        self.piece_size = 2 * 1024 * 1024
        self.size = fixture.stat().st_size
        hashes = []
        with fixture.open("rb") as source:
            while data := source.read(self.piece_size):
                hashes.append(hashlib.sha1(data).digest())
        self.info = encode({"length": self.size, "name": fixture.name, "piece length": self.piece_size,
                            "pieces": b"".join(hashes), "private": 1})
        self.info_hash = hashlib.sha1(self.info).digest()
        self.uploaded = 0
        self.lock = threading.Lock()
        seed = self

        class Peer(socketserver.StreamRequestHandler):
            def handle(self):
                self.request.settimeout(120)
                try:
                    self.serve()
                except (ConnectionError, OSError, ValueError, IndexError, KeyError):
                    pass

            def send(self, payload):
                self.wfile.write(struct.pack("!I", len(payload)) + payload)
                self.wfile.flush()

            def serve(self):
                handshake = self.rfile.read(68)
                if len(handshake) != 68 or handshake[:20] != b"\x13BitTorrent protocol" or handshake[28:48] != seed.info_hash:
                    return
                self.wfile.write(b"\x13BitTorrent protocol" + b"\0\0\0\0\0\x10\0\0" + seed.info_hash + b"-SC0001-" + os.urandom(12))
                self.wfile.flush()
                self.send(b"\x14\0" + encode({"m": {"ut_metadata": 1}, "metadata_size": len(seed.info)}))
                count = len(hashes)
                bits = bytearray((count + 7) // 8)
                for i in range(count):
                    bits[i // 8] |= 1 << (7 - i % 8)
                self.send(b"\x05" + bits)
                self.send(b"\x01")  # Unchoke.
                metadata_id = 1
                with fixture.open("rb") as source:
                    while header := self.rfile.read(4):
                        if len(header) != 4:
                            return
                        length = struct.unpack("!I", header)[0]
                        if length > 1024 * 1024:
                            return
                        if not length:
                            continue
                        message = self.rfile.read(length)
                        if len(message) != length:
                            return
                        if message[0] == 20 and message[1] == 0:
                            extension, _ = decode(message[2:])
                            metadata_id = extension.get(b"m", {}).get(b"ut_metadata", 1)
                        elif message[0] == 20 and message[1] == 1:
                            request, _ = decode(message[2:])
                            if request[b"msg_type"] == 0:
                                piece = request[b"piece"]
                                part = seed.info[piece * 16384:(piece + 1) * 16384]
                                self.send(bytes([20, metadata_id]) + encode({
                                    "msg_type": 1, "piece": piece, "total_size": len(seed.info)}) + part)
                        elif message[0] == 6 and len(message) == 13:
                            piece, begin, amount = struct.unpack("!III", message[1:])
                            offset = piece * seed.piece_size + begin
                            if amount > 16384 or begin + amount > seed.piece_size or offset + amount > seed.size:
                                return
                            source.seek(offset)
                            block = source.read(amount)
                            self.send(b"\x07" + struct.pack("!II", piece, begin) + block)
                            with seed.lock:
                                seed.uploaded += len(block)

        class Server(socketserver.ThreadingTCPServer):
            daemon_threads = True

        self.server = Server(("127.0.0.1", 0), Peer)
        self.address = f"127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()
