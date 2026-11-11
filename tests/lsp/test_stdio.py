"""Exercise the actual stdio server, including edits, URIs and dependency isolation."""
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import unittest
from urllib.parse import unquote, urlparse


class Client:
    def __init__(self, root):
        self.root = root
        self.diagnostics = {}
        self.versions = {}
        self.registrations = []
        self.next_id = 0
        self.incoming = queue.Queue()
        self.stderr = tempfile.TemporaryFile()
        env = dict(os.environ, ARM_GCC="/missing/arm-none-eabi-gcc", EMELINO_PYTHON="/missing/python")
        for key in ("STM32CUBE_F4", "NRFX_ROOT", "CMSIS_ROOT", "PICO_SDK_PATH", "picotool_DIR"):
            env.pop(key, None)
        self.process = subprocess.Popen([os.environ["EMELINO_TEST_BIN"], "lsp"], cwd=root,
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr, env=env)
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()
        result = self.request("initialize", {"processId": None,
            "workspaceFolders": [{"uri": root.as_uri(), "name": "device"}],
            "capabilities": {"workspace": {"didChangeWatchedFiles": {"dynamicRegistration": True}}}})
        assert result["capabilities"]["positionEncoding"] == "utf-16"
        assert result["serverInfo"]["name"] == "emelino"
        self.notify("initialized", {})
        self.sync()

    def read(self):
        try:
            while True:
                header = self.process.stdout.readline()
                if not header:
                    raise EOFError("server closed stdout")
                if not header.startswith(b"Content-Length: "):
                    raise ValueError("non-LSP stdout: " + repr(header))
                size = int(header.split(b":", 1)[1])
                while self.process.stdout.readline().strip():
                    pass
                self.incoming.put(json.loads(self.process.stdout.read(size)))
        except Exception as error:
            self.incoming.put(error)

    def send(self, message):
        data = json.dumps(dict(jsonrpc="2.0", **message), ensure_ascii=False).encode()
        self.process.stdin.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
        self.process.stdin.flush()

    def notify(self, method, params):
        self.send({"method": method, "params": params})

    def request(self, method, params, error=False):
        self.next_id += 1
        request_id = self.next_id
        self.send({"id": request_id, "method": method, "params": params})
        while True:
            try:
                message = self.incoming.get(timeout=10)
            except queue.Empty:
                self.stderr.seek(0)
                raise AssertionError("LSP timeout: " + self.stderr.read().decode())
            if isinstance(message, Exception):
                self.stderr.seek(0)
                raise AssertionError(f"{message}: " + self.stderr.read().decode())
            if message.get("method") == "textDocument/publishDiagnostics":
                params = message["params"]
                self.diagnostics[params["uri"]] = params["diagnostics"]
                self.versions[params["uri"]] = params.get("version")
            elif message.get("method") == "client/registerCapability":
                self.registrations.append(message["params"])
                self.send({"id": message["id"], "result": None})
            elif message.get("id") == request_id:
                if error:
                    return message.get("error")
                if "error" in message:
                    raise AssertionError(message["error"])
                return message.get("result")

    def query(self, method, path, source, name, after=False):
        offset = source.index(name) + (len(name) if after else 1)
        before = source[:offset]
        position = {"line": before.count("\n"),
                    "character": len(before.rsplit("\n", 1)[-1].encode("utf-16-le")) // 2}
        return self.request("textDocument/" + method,
                            {"textDocument": {"uri": path.as_uri()}, "position": position})

    def sync(self):
        self.request("textDocument/hover", {"textDocument": {"uri": (self.root / "src/main.emel").as_uri()},
                                          "position": {"line": 0, "character": 0}})

    def open(self, path, source):
        self.notify("textDocument/didOpen", {"textDocument": {"uri": path.as_uri(),
            "languageId": "emela", "version": 1, "text": source}})
        self.sync()

    def change(self, path, source, version):
        self.notify("textDocument/didChange", {"textDocument": {"uri": path.as_uri(), "version": version},
                                              "contentChanges": [{"text": source}]})
        self.sync()

    def errors(self, path=None):
        groups = [self.diagnostics.get(path.as_uri(), [])] if path else self.diagnostics.values()
        return [d for group in groups for d in group if d.get("severity") == 1]

    def close(self):
        if self.process.poll() is None:
            self.request("shutdown", None)
            self.notify("exit", None)
            self.process.stdin.close()
            self.process.wait(timeout=5)
            assert self.process.returncode == 0
        self.reader.join(timeout=2)
        self.process.stdout.close()
        self.stderr.close()


class LspTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="emelino LSP 日本語 ' # ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "device"
        subprocess.run([os.environ["EMELINO_TEST_BIN"], "new", str(self.root), "--board", "raspberry-pi-pico"],
                       check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.main = self.root / "src/main.emel"
        self.client = None
        self.addCleanup(self.cleanup)

    def cleanup(self):
        if self.client:
            try:
                self.client.close()
            finally:
                if self.client.process.poll() is None:
                    self.client.process.kill()
                    self.client.process.wait()

    def start(self):
        self.client = Client(self.root)
        return self.client

    def test_gpio_defaults_hover_completion_and_library_definition(self):
        source = self.main.read_text()
        client = self.start()
        client.open(self.main, source)
        self.assertEqual(client.errors(), [])
        self.assertTrue(client.registrations)
        hover = client.query("hover", self.main, source, "configure")
        self.assertIn("configure", hover["contents"]["value"])
        target = client.query("definition", self.main, source, "configure")
        library = Path(unquote(urlparse(target["uri"]).path))
        self.assertTrue(library.is_file())
        self.assertEqual(library.name, "gpio.emel")
        self.assertIn("fn configure", library.read_text().splitlines()[target["range"]["start"]["line"]])
        error = client.request("textDocument/rename", {"textDocument": {"uri": target["uri"]},
            "position": target["range"]["start"], "newName": "changed"}, error=True)
        self.assertEqual(error["code"], -32601)
        edited = source.replace("gpio.configure", "gpio.conf")
        client.change(self.main, edited, 2)
        items = client.query("completion", self.main, edited, "gpio.conf", after=True)["items"]
        names = {item["label"] for item in items}
        self.assertTrue({"configure", "read", "write", "toggle"} <= names, names)
        self.assertFalse((self.root / "target/emelino/raspberry-pi-pico/firmware").exists())

    def test_user_modules_unsaved_type_errors_recovery_and_close(self):
        math = self.root / "src/math.emel"
        math.write_text('## Returns the answer.\npub fn answer() -> Int { 42 }\n')
        source = 'import Math\npub fn main() -> Int { Math.answer() }\n'
        self.main.write_text(source)
        client = self.start()
        client.open(self.main, source)
        hover = client.query("hover", self.main, source, "answer")
        self.assertIn("Int", hover["contents"]["value"])
        self.assertIn("Returns the answer", hover["contents"]["value"])
        target = client.query("definition", self.main, source, "answer")
        self.assertEqual(target["uri"], math.as_uri())
        client.open(math, 'pub fn answer() -> Int { "wrong" }\n')
        self.assertTrue(client.errors(math))
        client.change(math, 'pub fn answer() -> Int { 7 }\n', 2)
        self.assertEqual(client.errors(), [])
        client.change(math, 'pub fn answer() -> Int { "stale" }\n', 1)
        self.assertEqual(client.errors(), [])
        client.notify("textDocument/didClose", {"textDocument": {"uri": math.as_uri()}})
        client.sync()
        self.assertEqual(client.errors(), [])
        edited = source.replace("Math.answer()", "Math.an")
        client.change(self.main, edited, 2)
        names = {i["label"] for i in client.query("completion", self.main, edited, "Math.an", after=True)["items"]}
        self.assertIn("answer", names)

    def test_syntax_errors_keep_partial_analysis_and_clear_old_diagnostics(self):
        source = 'pub fn main() -> Int {\n  visible = 42\n  vis\n}\n'
        self.main.write_text(source)
        client = self.start()
        client.open(self.main, source)
        self.assertTrue(client.errors(self.main))
        # Query precisely after the partial local name.
        items = client.request("textDocument/completion", {"textDocument": {"uri": self.main.as_uri()},
            "position": {"line": 2, "character": 5}})["items"]
        self.assertIn("visible", {i["label"] for i in items})
        client.change(self.main, 'pub fn main() -> Int {', 2)
        self.assertTrue(client.errors(self.main))
        client.change(self.main, 'pub fn main() -> Int { 0 }\n', 3)
        self.assertEqual(client.errors(), [])
        self.assertEqual(client.versions[self.main.as_uri()], 3)

    def test_unsaved_new_module_and_manifest_changes_and_disk_watcher(self):
        client = self.start()
        source = 'import Helper\npub fn main() -> Int { Helper.answer() }\n'
        client.open(self.main, source)
        self.assertTrue(client.errors(self.main))
        helper = self.root / "src/helper.emel"
        client.open(helper, 'pub fn answer() -> Int { 42 }\n')
        self.assertEqual(client.errors(), [])
        self.assertFalse(helper.exists())
        manifest = self.root / "Emelino.toml"
        baseline = manifest.read_text()
        broken = baseline.replace("src/main.emel", "src/missing.emel")
        client.open(manifest, broken)
        self.assertIn("entry is missing", str(client.errors(manifest)))
        client.change(manifest, baseline, 2)
        self.assertEqual(client.errors(), [])
        client.notify("textDocument/didClose", {"textDocument": {"uri": manifest.as_uri()}})
        client.sync()
        manifest.write_text(broken)
        client.notify("workspace/didChangeWatchedFiles", {"changes": [{"uri": manifest.as_uri(), "type": 2}]})
        client.sync()
        self.assertTrue(client.errors(manifest))
        manifest.write_text(baseline)
        client.notify("workspace/didChangeWatchedFiles", {"changes": [{"uri": manifest.as_uri(), "type": 2}]})
        client.sync()
        self.assertEqual(client.errors(), [])

    def test_utf16_columns_after_an_emoji(self):
        source = 'pub fn main() -> String { String.concat(["😀", helper()]) }\nfn helper() -> String { "ok" }\n'
        self.main.write_text(source)
        client = self.start()
        client.open(self.main, source)
        self.assertEqual(client.errors(), [])
        hover = client.query("hover", self.main, source, "helper")
        self.assertIn("String", hover["contents"]["value"])
        target = client.query("definition", self.main, source, "helper")
        self.assertEqual(target["uri"], self.main.as_uri())
        self.assertEqual(target["range"]["start"]["line"], 1)

    def test_import_and_module_completion_and_local_scope(self):
        client = self.start()
        source = 'import Embedded.G\npub fn main() -> Int { 0 }\n'
        client.open(self.main, source)
        names = {i["label"] for i in client.query("completion", self.main, source, "Embedded.G", after=True)["items"]}
        self.assertIn("Gpio", names)
        source = 'import Embedded.Gpio\npub fn main() -> Int { Gpio.Out }\n'
        client.change(self.main, source, 2)
        names = {i["label"] for i in client.query("completion", self.main, source, "Gpio.Out", after=True)["items"]}
        self.assertIn("OutputPushPull", names)
        self.assertNotIn("configure", names)
        source = 'fn other() -> Int {\n  secret = 1\n  secret\n}\npub fn main() -> Int {\n  visible = 42\n  vis\n}\n'
        client.change(self.main, source, 3)
        items = client.request("textDocument/completion", {"textDocument": {"uri": self.main.as_uri()},
            "position": {"line": 6, "character": 5}})["items"]
        names = {i["label"] for i in items}
        self.assertIn("visible", names)
        self.assertNotIn("secret", names)


if __name__ == "__main__":
    unittest.main()
