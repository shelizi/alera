"""Explicit-only native Session acceptance against a disposable Rust Host.

Claude uses the normally authenticated CLI, with private per-launch settings,
no tools, and a bounded model budget; it creates only new test sessions.
Codex uses its actual installed CLI/Hook engine with an isolated CODEX_HOME
and a deterministic local Responses fixture (no account access or model bill).
Both route genuine provider hooks through the production Rust relay.
These are successive CLI invocations on one registered Host PTY per provider,
not same-process interactive slash-command acceptance.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import uuid

from mcp_ui_agent_desktop_e2e import HostRpc

ROOT = Path(__file__).resolve().parents[2]
EVENTS = {"SessionStart": "session_start", "UserPromptSubmit": "user_prompt_submit",
          "Stop": "stop", "SessionEnd": "session_end"}


def run(args, *, env=None, cwd=None, timeout=90):
    return subprocess.run([str(x) for x in args], env=env, cwd=cwd, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8",
                          errors="replace", timeout=timeout)


def wait_until(fn, seconds=12):
    deadline = time.monotonic() + seconds
    last = None
    while time.monotonic() < deadline:
        try:
            last = fn()
            if last:
                return last
        except (OSError, RuntimeError, KeyError):
            pass
        time.sleep(.15)
    raise AssertionError("bounded condition not satisfied")


def clean_env():
    return {k: v for k, v in os.environ.items() if not k.startswith("ALERA_")}


def hook_settings(base, cli, provider):
    hooks = {}
    for event in EVENTS:
        path = base / (provider + "-" + event + ".cmd")
        relay = ('"' + sys.executable + '" -B "' + str(Path(__file__).resolve()) +
                 '" --relay "' + str(cli) + '" "' + str(base/"hook-trace.jsonl") + '"'
                 if provider == "codex" else '"' + str(cli) + '" agent-hook-relay')
        path.write_text('@echo off\nsetlocal\nset "ALERA_AGENT_TYPE=' + provider +
                        '"\nset "ALERA_AGENT_HOOK_EVENT=' + event +
                        '"\n' + relay + '\nexit /b 0\n', encoding="utf-8")
        command = ('MSYS2_ARG_CONV_EXCL="*" cmd.exe /d /s /c call "' + path.as_posix() + '"'
                   if provider == "claude" else
                   'cmd /d /s /c "call ""' + str(path) + '"""')
        hooks[event] = [{"hooks": [{"type": "command", "command": command}]}]
    return hooks


class ResponseFixture(BaseHTTPRequestHandler):
    """Minimum streaming Responses API fixture: only known test marker output."""
    protocol_version = "HTTP/1.1"
    def log_message(self, *args):
        pass

    def do_POST(self):
        data = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        if not self.path.split("?")[0].endswith("/responses"):
            self.send_error(404)
            return
        # No request bodies are logged or stored.
        text = self.server.marker
        item = {"id":"msg_fixture", "type":"message", "role":"assistant", "status":"completed",
                "content":[{"type":"output_text","text":text,"annotations":[]}]}
        response = {"id":"resp_" + uuid.uuid4().hex, "object":"response",
                    "created_at":int(time.time()), "model":"gpt-5.6",
                    "status":"completed", "output":[item],
                    "usage":{"input_tokens":10,"output_tokens":8,"total_tokens":18,
                             "input_tokens_details":{"cached_tokens":0},
                             "output_tokens_details":{"reasoning_tokens":0}}}
        events = [
            {"type":"response.created","response":{**response,"status":"in_progress","output":[]}},
            {"type":"response.output_item.added","output_index":0,
             "item":{**item,"status":"in_progress","content":[]}},
            {"type":"response.content_part.added","item_id":item["id"],"output_index":0,
             "content_index":0,"part":{"type":"output_text","text":"","annotations":[]}},
            {"type":"response.output_text.delta","item_id":item["id"],"output_index":0,
             "content_index":0,"delta":text},
            {"type":"response.output_text.done","item_id":item["id"],"output_index":0,
             "content_index":0,"text":text},
            {"type":"response.content_part.done","item_id":item["id"],"output_index":0,
             "content_index":0,"part":item["content"][0]},
            {"type":"response.output_item.done","output_index":0,"item":item},
            {"type":"response.completed","response":response},
        ]
        body = "".join("event: " + event["type"] + "\ndata: " +
                       json.dumps({**event, "sequence_number":i}) + "\n\n"
                       for i,event in enumerate(events)).encode()
        self.send_response(200)
        self.send_header("Content-Type","text/event-stream")
        self.send_header("Content-Length",str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--provider", choices=["claude", "codex"], required=True)
    opts = parser.parse_args()
    cli = opts.cli.resolve()
    if os.name != "nt" or not cli.is_file():
        raise RuntimeError("requires Windows and a freshly built Alera CLI")
    build = ROOT / "build"
    build.mkdir(exist_ok=True)
    provider = opts.provider
    phase = "preflight"
    result = {"provider":provider, "checks":[], "passed":False}
    with tempfile.TemporaryDirectory(prefix="native-context-", dir=build, ignore_cleanup_errors=True) as name:
        base = Path(name)
        runtime = base / "runtime"
        workspace = base / "workspace"
        workspace.mkdir()
        home = base / "host-home"
        home.mkdir()
        env = clean_env()
        for key in ("CLAUDE_CONFIG_DIR","CODEX_HOME","COPILOT_HOME","GROK_HOME",
                    "OPENCODE_CONFIG_DIR","PI_CODING_AGENT_DIR","CCS_HOME","GEMINI_CLI_HOME"):
            env.pop(key,None)
        env.update(HOME=str(home), USERPROFILE=str(home),
                   APPDATA=str(home/"AppData"/"Roaming"), LOCALAPPDATA=str(home/"AppData"/"Local"))
        Path(env["APPDATA"]).mkdir(parents=True)
        Path(env["LOCALAPPDATA"]).mkdir(parents=True)
        project, ws, handle = "native-context-project", "native-context-workspace", "native-context-pty"
        def ctl(*args):
            proc = run([cli,*args],env=env,timeout=30)
            if proc.returncode:
                raise RuntimeError("private Host CLI command failed: " + str(args[0]))
            return json.loads(proc.stdout) if proc.stdout.strip() else {}
        host = None
        server = None
        started = False
        try:
            phase = "start_private_host"
            started = True
            # A detached Host can inherit a capture pipe on Windows; use
            # DEVNULL so the CLI launcher can exit without waiting for Host EOF.
            proc = subprocess.Popen([str(cli),"runtime","--runtime-dir",str(runtime),"--json","start"],
                env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,
                creationflags=getattr(subprocess,"CREATE_NO_WINDOW",0))
            try:
                assert proc.wait(timeout=30)==0
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=5)
                raise
            wait_until(lambda:(runtime/"agent-hooks"/"endpoint.cmd").is_file(),25)
            print(json.dumps({"phase":"private_host_started","provider":provider}),flush=True)
            phase = "register_private_project"
            ctl("project","--runtime-dir",runtime,"--json","add","--id",project,"--name",
                "Disposable native context","--repo-path",workspace,"--kind","folder")
            ctl("workspace","--runtime-dir",runtime,"--json","register","--id",ws,
                "--project-id",project,"--name","Private","--path",workspace,"--kind","main")
            phase = "spawn_private_pty"
            host = HostRpc(runtime)
            now = datetime.now(timezone.utc).isoformat()
            host.request("tab.upsert", {"id":handle,"workspaceId":ws,"kind":"terminal",
                "title":"Private test","createdAt":now,"updatedAt":now,
                "payload":{"terminalSessionId":handle,"agentType":provider,"spawnOnCreate":True}})
            wait_until(lambda:any(x.get("handle")==handle and x.get("running")
                for x in ctl("terminal","--runtime-dir",runtime,"--json","list").get("items",[])))
            phase = "enable_private_hooks"
            host.request("runtimeSettings.update",{"agentStatusHooks":{provider:True},
                "aiTextGeneration":{"enabled":False,"autoGenerateAgentTitles":False,"autoSummarizeAgentReports":False}})
            result["checks"].append("private_host_and_pty")
            print(json.dumps({"phase":"private_host_and_pty_ready","provider":provider}),flush=True)
            phase = "prepare_provider"
            actual = clean_env()
            for line in (runtime/"agent-hooks"/"endpoint.cmd").read_text(encoding="utf-8").splitlines():
                if line.startswith("set ALERA_AGENT_HOOK_") and "=" in line:
                    key,val = line[4:].split("=",1)
                    actual[key] = val.strip()
            actual.update(ALERA_AGENT_TYPE=provider,ALERA_TERMINAL_SESSION_ID=handle,
                          ALERA_WORKSPACE_ID=ws,ALERA_TAB_ID=handle,ALERA_AGENT_HOOK_VERSION="1")
            if not actual.get("ALERA_AGENT_HOOK_TOKEN") or not actual.get("ALERA_AGENT_HOOK_PORT"):
                raise AssertionError("private hook endpoint unavailable")
            hooks = hook_settings(base, cli, provider)
            if provider == "claude":
                exe = shutil.which("claude.exe")
                assert exe
                settings = base/"claude-settings.json"
                settings.write_text(json.dumps({"hooks":hooks}),encoding="utf-8")
                common = [exe,"--setting-sources","","--settings",settings,"--strict-mcp-config",
                          "--tools","","--permission-prompts","none","--max-turns","2",
                          "--max-budget-usd","0.10","--output-format","stream-json","--verbose","--print"]
                a,b = str(uuid.uuid4()),str(uuid.uuid4())
            else:
                shim = Path(shutil.which("codex.cmd") or "")
                node = shutil.which("node.exe")
                js = shim.parent/"node_modules"/"@openai"/"codex"/"bin"/"codex.js"
                assert node and js.is_file()
                codex = base/"codex-home"
                codex.mkdir()
                actual.update(CODEX_HOME=str(codex))
                # Use no stored user credentials, config, plugins, or external services.
                actual.pop("OPENAI_API_KEY",None)
                actual.pop("CODEX_API_KEY",None)
                server = ThreadingHTTPServer(("127.0.0.1",0),ResponseFixture)
                server.marker = ""
                threading.Thread(target=server.serve_forever,daemon=True).start()
                hp = codex/"hooks.json"
                hp.write_text(json.dumps({"hooks":hooks}),encoding="utf-8")
                cfg = ['model = "gpt-5.6"','model_provider = "alera_fixture"',
                       'approval_policy = "never"','sandbox_mode = "read-only"',
                       '[features]','hooks = true',
                       '[model_providers.alera_fixture]','name = "Local test fixture"',
                       'base_url = "http://127.0.0.1:'+str(server.server_port)+'/v1"',
                       'wire_api = "responses"','requires_openai_auth = false']
                for event,label in EVENTS.items():
                    command = hooks[event][0]["hooks"][0]["command"]
                    identity = {"event_name":label,"hooks":[{"async":False,"command":command,
                                                            "timeout":600,"type":"command"}]}
                    digest = hashlib.sha256(json.dumps(identity,sort_keys=True,ensure_ascii=False,
                                                        separators=(",",":")).encode()).hexdigest()
                    key = str(hp.resolve())+":"+label+":0:0"
                    cfg.extend(["[hooks.state."+json.dumps(key)+"]","enabled = true",
                                'trusted_hash = "sha256:'+digest+'"'])
                (codex/"config.toml").write_text("\n".join(cfg)+"\n",encoding="utf-8")
                common = [node,js,"--no-daemon","exec","--skip-git-repo-check","--json","--color","never"]
                a=b=None
            nonce = uuid.uuid4().hex[:8].upper()
            def turn(label, native=None, resume=False):
                marker = "ALERA_"+nonce+"_"+label
                prompt = "This is a disposable session test. Do not use tools or edit files. Reply exactly "+marker+"."
                if provider == "claude":
                    args = common+(["--resume",native] if resume else ["--session-id",native])+[prompt]
                else:
                    server.marker = marker
                    args = ([node,js,"--no-daemon","exec","resume","--skip-git-repo-check","--json",
                             native,prompt] if resume else common+[prompt])
                proc = run(args,env=actual,cwd=workspace,timeout=75)
                lines=[]
                for line in proc.stdout.splitlines():
                    try:
                        value=json.loads(line)
                        if isinstance(value,dict): lines.append(value)
                    except ValueError:
                        pass
                if provider == "claude":
                    good=any(x.get("type")=="result" and not x.get("is_error")
                             and marker in str(x.get("result","")) for x in lines)
                    ids=[x.get("session_id") for x in lines if x.get("type")=="system" and x.get("subtype")=="init"]
                else:
                    good=any(x.get("type")=="item.completed" and marker in str(x.get("item",{}).get("text","")) for x in lines)
                    ids=[x.get("thread_id") for x in lines if x.get("type")=="thread.started"]
                if proc.returncode or not good or not ids:
                    # Print only safe counts and categorized errors, never account/provider content.
                    low=proc.stderr.lower()
                    hints=[key for key in ("unauthorized","authentication","not logged","invalid","hook","timeout",
                                           "connection","model","error","trust","quota","sandbox") if key in low]
                    print(json.dumps({"phase":label,"exit":proc.returncode,"jsonEvents":len(lines),
                                      "resultFound":good,"threadIdFound":bool(ids),"diagnosticCategories":hints}),flush=True)
                    raise AssertionError("native CLI turn failed")
                uuid.UUID(ids[-1])
                assert native is None or ids[-1] == native
                result["checks"].append("actual_cli_"+label)
                def context():
                    tab=host.request("tab.find",{"id":handle})["payload"]
                    token=tab.get("agentTitleConversationId")
                    if not token: return None
                    ctx=host.request("agentContext.get",{"workspaceId":ws,"tabId":handle,
                        "sessionId":handle,"conversationId":token}).get("contextOverview",{})
                    return (token,ctx) if marker in ctx.get("progress","") else None
                try:
                    token,ctx=wait_until(context)
                except AssertionError:
                    if provider == "codex":
                        trace=base/"hook-trace.jsonl"
                        if trace.is_file():
                            print(json.dumps({"hookTrace":[json.loads(line) for line in trace.read_text(encoding="utf-8").splitlines()]}),flush=True)
                        tab=host.request("tab.find",{"id":handle})["payload"]
                        current=host.request("agentContext.get",{"workspaceId":ws,"tabId":handle,
                            "sessionId":handle,"conversationId":tab["agentTitleConversationId"]}).get("contextOverview",{})
                        print(json.dumps({"phase":label,"hostGoal":current.get("goal"),"hostReport":current.get("progress"),
                                          "hostTimelineCount":len(current.get("timeline",[]))}),flush=True)
                    raise
                result["checks"].append("host_checkpoint_"+label)
                print(json.dumps({"phase":label,"nativeIdMatches":native is None or ids[-1]==native,
                                  "hostReportRecovered":True,"timelineCount":len(ctx.get("timeline",[]))}),flush=True)
                return ids[-1],token,ctx,marker
            phase="A"
            a,ta,ca,ma=turn("A",a)
            phase="B"
            b,tb,cb,mb=turn("B",b)
            assert a!=b and ta!=tb and ma not in json.dumps(cb)
            assert ma in ca.get("goal","") and mb in cb.get("goal","")
            result["checks"].append("independent_A_B")
            # A UI still holding A's old activation cannot request B's Context.
            try:
                host.request("agentContext.get",{"workspaceId":ws,"tabId":handle,
                    "sessionId":handle,"conversationId":ta})
            except RuntimeError:
                result["checks"].append("old_activation_rejected")
            else:
                raise AssertionError("stale A activation exposed current B")
            phase="resume_A"
            ra,tr,cr,mr=turn("RESUME_A",a,True)
            assert ra==a and tr not in (ta,tb)
            assert ma in cr.get("goal","") and mb not in json.dumps(cr)
            reports=[item.get("text","") for item in cr.get("timeline",[]) if item.get("kind")=="report"]
            assert any(ma in item for item in reports) and any(mr in item for item in reports)
            result["checks"].append("resume_restores_A_not_B")
            if provider == "codex":
                trace=[json.loads(line) for line in (base/"hook-trace.jsonl").read_text(encoding="utf-8").splitlines()]
                starts=[item for item in trace if item.get("event")=="SessionStart"]
                assert len(starts)==3 and starts[2].get("source")=="resume"
                assert starts[2].get("id")==starts[0].get("id")
                assert starts[2].get("samePhysicalFile") and starts[2].get("metaIdMatches")
                result["checks"].append("real_codex_resume_same_physical_rollout")
                result["windowsExtendedPathOnResume"]=starts[2].get("extendedWindowsPath",False)
            result["passed"]=True
        except Exception as exc:
            result.update(phase=phase,error=type(exc).__name__)
            # Error diagnostics are deliberately limited to test stages.
        finally:
            if host:
                host.close()
            if server:
                server.shutdown()
                server.server_close()
            if started:
                proc=subprocess.run([str(cli),"runtime","--runtime-dir",str(runtime),"--json","stop","--force"],
                    env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=25)
                result["privateHostStopped"]=proc.returncode==0
                if not result["privateHostStopped"]:
                    result["passed"]=False
        print(json.dumps(result),flush=True)
    return 0 if result["passed"] else 1


def relay_trace(cli, trace):
    """Record only metadata from the local synthetic Codex fixture, then relay."""
    data=sys.stdin.buffer.read()
    try:
        raw=json.loads(data)
        path=Path(raw["transcript_path"]) if raw.get("transcript_path") else None
        event={"event":raw.get("hook_event_name"),"id":raw.get("session_id"),
               "source":raw.get("source"),"transcriptName":path.name if path else None,
               "transcriptExists":path.is_file() if path else False,
               "pathHash":hashlib.sha256(str(raw.get("transcript_path")).encode()).hexdigest()[:12],
               "resolvedPathHash":hashlib.sha256(str(path.resolve()).lower().encode()).hexdigest()[:12] if path else None,
               "hasPrompt":bool(raw.get("prompt")), "hasReport":bool(raw.get("last_agent_message") or raw.get("last_assistant_message"))}
        if path and path.is_file():
            with path.open(encoding="utf-8") as fp:
                meta=json.loads(fp.readline())
            # This trace only runs against the private Codex fixture.
            event["fixturePath"]=str(path.resolve().relative_to(Path(trace).parent.resolve())) if path.resolve().is_relative_to(Path(trace).parent.resolve()) else "outside_fixture"
            event["originalPath"]=str(path.relative_to(Path(trace).parent)) if path.is_relative_to(Path(trace).parent) else "different_root_or_alias"
            resolved=path.resolve()
            event["tempRelative"]=str(resolved.relative_to(Path(tempfile.gettempdir()).resolve())) if resolved.is_relative_to(Path(tempfile.gettempdir()).resolve()) else None
            event["parentNames"]=[part.name for part in list(resolved.parents)[:5]]
            event["size"]=path.stat().st_size
            known=Path(trace).parent/"codex-home"/"sessions"/path.parent.parent.parent.name/path.parent.parent.name/path.parent.name/path.name
            event["samePhysicalFile"]=known.is_file() and os.path.samefile(known,path)
            event["extendedWindowsPath"]=str(path).startswith("\\\\?\\")
            event["resolvedExtendedWindowsPath"]=str(resolved).startswith("\\\\?\\")
            event["firstLineHash"]=hashlib.sha256(json.dumps(meta,sort_keys=True).encode()).hexdigest()[:12]
            event["metaType"]=meta.get("type")
            event["metaIdMatches"]=meta.get("payload",{}).get("id")==raw.get("session_id")
            event["metaSessionIdMatches"]=meta.get("payload",{}).get("session_id") in (None,raw.get("session_id"))
            event["metaCwdMatches"]=str(meta.get("payload",{}).get("cwd","")).replace("\\","/").lower()==str(raw.get("cwd","")).replace("\\","/").lower()
        with Path(trace).open("a",encoding="utf-8") as fp:
            fp.write(json.dumps(event)+"\n")
    except Exception:
        pass
    return subprocess.run([cli,"agent-hook-relay"],input=data,stdout=subprocess.DEVNULL,
                          stderr=subprocess.DEVNULL,timeout=6).returncode


if __name__=="__main__":
    if len(sys.argv)==4 and sys.argv[1]=="--relay":
        sys.exit(relay_trace(sys.argv[2],sys.argv[3]))
    sys.exit(main())
