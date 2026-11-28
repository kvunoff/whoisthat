#!/usr/bin/env python3
"""Exercise a real, capless daemon over raw IPC with a local SOCKS upstream."""
import json
import os
from pathlib import Path
import shutil
import socket
import socketserver
import struct
import subprocess
import tempfile
import threading
import time


def recv_exact(sock, size):
    result = b""
    while len(result) < size:
        chunk = sock.recv(size - len(result))
        if not chunk:
            raise EOFError()
        result += chunk
    return result

seen = threading.Event()
class Upstream(socketserver.BaseRequestHandler):
    def handle(self):
        sock = self.request
        sock.settimeout(4)
        version, modes = recv_exact(sock, 2)
        assert version == 5 and 0 in recv_exact(sock, modes)
        sock.sendall(b"\x05\x00")
        version, command, _, address_type = recv_exact(sock, 4)
        assert version == 5 and command == 1
        if address_type == 3:
            size = recv_exact(sock, 1)[0]
        else:
            size = {1: 4, 4: 16}[address_type]
        recv_exact(sock, size + 2)
        sock.sendall(b"\x05\x00\x00\x01" + bytes(6))
        request = b""
        while b"\r\n\r\n" not in request:
            request += recv_exact(sock, 1)
        assert b"GET /proof " in request, request
        seen.set()
        sock.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\nstandalone-core")

def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]

def attach(sockpath, process):
    for _ in range(300):
        if process.poll() is not None:
            raise RuntimeError(f"core exited with {process.returncode}")
        wire = socket.socket(socket.AF_UNIX)
        wire.settimeout(30)
        try:
            wire.connect(str(sockpath))
            return wire
        except OSError:
            wire.close()
            time.sleep(.1)
    raise TimeoutError("IPC socket")


def send(wire, name, data, request_id, result=False):
    frame = dict(msg=name, data=data, id=request_id)
    if result:
        frame["want_result"] = True
    body = json.dumps(frame).encode()
    wire.sendall(struct.pack(">I", len(body)) + body)


def receive(wire):
    size = struct.unpack(">I", recv_exact(wire, 4))[0]
    return json.loads(recv_exact(wire, size))


def reply(wire, name, request_id, ignored_warnings=()):
    while True:
        frame = receive(wire)
        if frame["msg"] == "operation-result" and frame.get("id") != request_id:
            raise AssertionError(f"unexpected operation result: {frame}")
        if frame.get("id") != request_id:
            continue
        if frame["msg"] == name:
            return frame["data"]
        if frame["msg"] == "warn" and frame["data"]["key"] not in ignored_warnings:
            raise AssertionError(frame)


def snapshot(wire, request_id="state"):
    send(wire, "get-application-state", {}, request_id)
    return reply(wire, "application-state", request_id)


def operation(wire, name, data, request_id, success=True):
    send(wire, name, data, request_id, result=True)
    result = reply(wire, "operation-result", request_id, (name + "-failed",))
    assert result["operation"] == name and result["ok"] == success, result
    return result


def await_runtime(wire):
    for _ in range(450):
        state = snapshot(wire)
        if state["xray_status"]["status"] == "ready":
            return state
        if state["xray_status"]["status"] == "error":
            raise RuntimeError(state["xray_status"])
        time.sleep(.2)
    raise TimeoutError("Xray runtime")


def run(directory, upstream):
    runtime = directory / "run"
    runtime.mkdir(mode=0o700)
    sockpath = runtime / "core.sock"
    config = directory / "config" / "whoisthat"
    config.mkdir(parents=True)
    source = os.environ.get("WHOISTHAT_E2E_XRAY_DIR")
    if source:
        source = Path(source)
        dest = directory / "data/whoisthat/runtimes/xray" / source.name
        shutil.copytree(source, dest)
    geo = os.environ.get("WHOISTHAT_E2E_GEO_DIR")
    if geo:
        shutil.copytree(geo, config / "geo")
    core_socks, core_http = free_port(), free_port()
    while core_http == core_socks:
        core_http = free_port()
    config_file = config / "config.json"
    config_file.write_text(json.dumps({
        "ipc-socket-path": str(sockpath), "socks-port": core_socks, "http-port": core_http,
    }))
    env = dict(os.environ, XDG_CONFIG_HOME=str(config.parent),
               XDG_DATA_HOME=str(directory / "data"), XDG_RUNTIME_DIR=str(runtime))
    env.pop("WHOISTHAT_TEST_MODE", None)
    core = Path(os.environ.get("WHOISTHAT_E2E_CORE", "core/core/whoisthat-core")).resolve()

    def start():
        return subprocess.Popen([str(core)], env=env)

    def stop(process):
        if process.poll() is None:
            process.terminate()
        assert process.wait(timeout=12) == 0

    process = start()
    try:
        with attach(sockpath, process) as wire, attach(sockpath, process) as legacy:
            state = await_runtime(wire)
            protocol_file = Path(__file__).resolve().parents[1] / "core/core/lib/version/protocol.txt"
            assert state["groups"] == [] and state["protocol_version"] == int(protocol_file.read_text()), state
            assert state["proxy_ports"] == {"socks_port": core_socks, "http_port": core_http}
            assert {"operation-result", "config-state"} <= set(state["ipc_features"])
            assert snapshot(legacy)["groups"] == []

            def add_profile(label):
                send(wire, "add-group", {"name": label, "subscription_url": ""}, "group")
                group = reply(wire, "group-added", "group")
                uri = f"socks://127.0.0.1:{upstream.server_address[1]}#{label}"
                send(wire, "add-profiles", {"group_id": group["id"], "uris": uri}, "add")
                profile = reply(wire, "profiles-added", "add")["profiles"][0]
                return {"id": profile["id"], "group_id": group["id"]}

            target = add_profile("Boot")
            manual = add_profile("Manual")
            assert operation(wire, "connect", {"profile": target}, "connect")["connection_status"]["connection"] == "connected"
            assert not operation(wire, "enable-tun", {}, "tun", success=False)["tun_enabled"]
            snapshot(legacy, "still-legacy")
            send(wire, "disconnect", None, "invalid-data", result=True)
            invalid = reply(wire, "operation-result", "invalid-data")
            assert invalid["error"]["key"] == "invalid-request"
            assert snapshot(wire)["connection-status"]["connection"] == "connected"
            send(wire, "set-autoconnect", {
                "enabled": True, "group_id": target["group_id"], "profile_id": target["id"], "mode": "proxy",
            }, "boot")
            assert reply(wire, "autoconnect-updated", "boot")["target"] == target

            # A valid runtime snapshot continues to describe the previous target when
            # persistence fails. A directory at the config path makes rename fail.
            config_file.rename(config / "saved.json")
            config_file.mkdir()
            try:
                send(wire, "set-autoconnect", {
                    "enabled": False, "group_id": manual["group_id"], "profile_id": manual["id"], "mode": "tun",
                }, "bad-write")
                frame = reply(wire, "warn", "bad-write")
                assert frame["key"] == "set-autoconnect-failed"
                assert reply(wire, "autoconnect-updated", "bad-write")["target"] == target
            finally:
                config_file.rmdir()
                (config / "saved.json").rename(config_file)

            # A legacy request still produces the broadcast used by old clients.
            send(legacy, "connect", {"profile": manual}, "legacy")
            while True:
                frame = receive(legacy)
                assert frame["msg"] != "operation-result", frame
                if frame["msg"] == "status-changed" and frame["data"]["connection"] == "connected":
                    profile = frame["data"]["profile"]
                    if (profile["group_id"], profile["id"]) == (manual["group_id"], manual["id"]):
                        break
            assert snapshot(wire)["autoconnect"]["target"] == target

            for _ in range(80):
                try:
                    proxy_sock = socket.create_connection(("127.0.0.1", core_http), timeout=5)
                    break
                except ConnectionRefusedError:
                    time.sleep(.05)
            else:
                raise TimeoutError("HTTP listener")
            with proxy_sock as proxy:
                proxy.sendall(b"GET http://192.0.2.1/proof HTTP/1.1\r\nHost: 192.0.2.1\r\nConnection: close\r\n\r\n")
                response = b"".join(iter(lambda: proxy.recv(4096), b""))
            assert b"standalone-core" in response and seen.is_set(), response
            assert operation(wire, "disconnect", {}, "stop")["connection_status"]["connection"] == "disconnected"
    finally:
        stop(process)

    process = start()
    try:
        with attach(sockpath, process) as wire:
            for _ in range(160):
                status = snapshot(wire)["connection-status"]
                if status["connection"] == "connected":
                    profile = status["profile"]
                    assert (profile["group_id"], profile["id"]) == (target["group_id"], target["id"])
                    return
                time.sleep(.1)
            raise TimeoutError("autoconnect")
    finally:
        stop(process)


if __name__ == "__main__":
    if os.getuid() == 0:
        raise SystemExit("run this capless test as a regular user")
    with tempfile.TemporaryDirectory(prefix="wit-core-e2e-") as work:
        with socketserver.ThreadingTCPServer(("127.0.0.1", 0), Upstream) as upstream:
            upstream.daemon_threads = True
            threading.Thread(target=upstream.serve_forever, daemon=True).start()
            try:
                run(Path(work), upstream)
            except Exception:
                log = Path(work) / "config/whoisthat/core.log"
                if log.exists():
                    print(log.read_text())
                raise
    print("Standalone core IPC, proxy traffic and autoconnect passed")
