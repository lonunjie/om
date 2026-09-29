#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import threading
import shutil
import subprocess
from contextlib import contextmanager
from datetime import datetime, timezone
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PROTOTYPE = "pwa-reference-v3"

def prototype_html(prototype_name=DEFAULT_PROTOTYPE):
    prototype = ROOT / "prototypes" / prototype_name
    html=(prototype/"index.html").read_text()
    css=(prototype/"styles.css").read_text()
    js=(prototype/"app.js").read_text()
    html=html.replace('<link rel="manifest" href="manifest.webmanifest">','')
    html=html.replace('<link rel="stylesheet" href="styles.css">',f'<style>{css}</style>')
    html=html.replace('<script src="app.js"></script>',f'<script>{js}</script>')
    return html

class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

@contextmanager
def local_server(directory: Path):
    def handler(*args, **kwargs):
        return QuietHandler(*args, directory=str(directory), **kwargs)
    httpd = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=httpd.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{httpd.server_port}/index.html"
    finally:
        httpd.shutdown()
        thread.join(timeout=2)


def assert_true(cond, msg):
    if not cond:
        raise AssertionError(msg)


def px_number(value: str) -> float:
    try:
        return float(value.replace("px", "").strip())
    except Exception:
        return 0.0


def launch_browser(browser_type, engine: str):
    default = Path(browser_type.executable_path)
    if default.exists():
        return browser_type.launch(headless=True)
    if engine == "chromium":
        system = shutil.which("chromium") or shutil.which("google-chrome") or shutil.which("google-chrome-stable")
        if system:
            return browser_type.launch(headless=True, executable_path=system, args=["--no-sandbox","--disable-gpu"])
    if engine == "firefox":
        system = shutil.which("firefox")
        if system:
            return browser_type.launch(headless=True, executable_path=system)
    return None


def test_engine(browser_type, html: str, engine: str):
    browser = launch_browser(browser_type, engine)
    if browser is None:
        return [{"name":"engine_availability","status":"not_run","detail":f"No runnable {engine} binary in this environment"}]
    context = browser.new_context(viewport={"width": 390, "height": 844})
    errors = []
    def make_page():
        p = context.new_page()
        p.set_default_timeout(2500)
        p.on("pageerror", lambda e: errors.append(str(e)))
        p.set_content(html, wait_until="load")
        return p
    page = make_page()

    checks = []
    def check(name, fn):
        try:
            fn()
            checks.append({"name": name, "status": "pass"})
        except Exception as e:
            checks.append({"name": name, "status": "fail", "detail": str(e)})

    def reset():
        nonlocal page
        page.close()
        page = make_page()

    check("default_single_tab_strip_hidden", lambda: assert_true(page.locator("#tabs").evaluate("e=>e.classList.contains('hidden')"), "tabs should be hidden by default"))

    def combo_semantics():
        inp = page.locator("#rootOmniInput")
        inp.focus()
        page.wait_for_timeout(20)
        assert_true(inp.get_attribute("role") == "combobox", "Omnibar input must be combobox")
        assert_true(inp.get_attribute("aria-autocomplete") == "list", "Omnibar must declare list autocomplete")
        assert_true(inp.get_attribute("aria-expanded") == "true", "focused Omnibar should expose suggestions")
        assert_true(page.locator("#omniResults").get_attribute("role") == "listbox", "popup must be listbox")
        assert_true(page.locator("#omniResults [role=option]").count() > 0, "listbox must contain options")
        active = inp.get_attribute("aria-activedescendant")
        assert_true(bool(active), "active descendant required while option is active")
        assert_true(page.locator(f"#{active}").get_attribute("aria-selected") == "true", "active descendant must be selected option")
    check("omnibar_combobox_listbox_semantics", combo_semantics)

    def keyboard_focus_stays_in_input():
        reset(); inp = page.locator("#rootOmniInput"); inp.focus(); page.wait_for_timeout(20)
        first = inp.get_attribute("aria-activedescendant")
        inp.press("ArrowDown")
        second = inp.get_attribute("aria-activedescendant")
        assert_true(first != second, "ArrowDown must change active option")
        assert_true(page.evaluate("document.activeElement.id") == "rootOmniInput", "DOM focus must remain in combobox")
        inp.press("Escape")
        assert_true(inp.get_attribute("aria-expanded") == "false", "Escape must collapse popup")
        assert_true(inp.get_attribute("aria-activedescendant") is None, "collapsed combobox must clear active descendant")
        assert_true(page.evaluate("document.activeElement.id") == "rootOmniInput", "Escape should keep focus in root Omnibar")
    check("omnibar_keyboard_active_descendant", keyboard_focus_stays_in_input)

    def options_are_atomic():
        reset(); page.locator("#rootOmniInput").focus(); page.wait_for_timeout(20)
        assert_true(page.locator("#omniResults [role=option] button, #omniResults [role=option] a, #omniResults [role=option] input").count() == 0, "options must not contain nested focusable action controls")
    check("omnibar_options_have_no_nested_controls", options_are_atomic)

    def enter_activation():
        reset(); inp=page.locator("#rootOmniInput"); inp.fill("Maria"); inp.press("ArrowDown"); inp.press("ArrowUp")
        # First result remains the stable bare entity result.
        active = inp.get_attribute("aria-activedescendant")
        assert_true("Maria Santos" in page.locator(f"#{active}").inner_text(), "bare Maria should resolve to Maria entity first")
        inp.press("Enter")
        assert_true(page.locator("#screenTitle").inner_text() == "Maria Santos", "Enter should activate selected entity")
    check("omnibar_enter_activation_stable_entity", enter_activation)

    def draft_and_focus_restore():
        reset(); page.locator(".chat-row").first.click();
        comp=page.locator("#composerInput"); comp.fill("draft survives")
        page.locator("#conversationOmniTrigger").click()
        assert_true(page.evaluate("document.activeElement.id") == "conversationOmniInput", "conversation Omnibar must take focus")
        page.keyboard.press("Escape")
        assert_true(comp.input_value() == "draft survives", "draft must survive Omni mode")
        assert_true(page.evaluate("document.activeElement.id") == "composerInput", "closing Omni mode must restore composer focus")
    check("conversation_omnibar_preserves_draft_and_focus", draft_and_focus_restore)

    def settings_focus_restore():
        reset(); b=page.locator("#settingsBtn"); b.click();
        assert_true(b.get_attribute("aria-expanded") == "true", "settings control should expose expansion")
        page.keyboard.press("Escape")
        assert_true(b.get_attribute("aria-expanded") == "false", "Escape should close settings")
        assert_true(page.evaluate("document.activeElement.id") == "settingsBtn", "settings dismissal must restore invoker focus")
    check("surface_focus_restoration", settings_focus_restore)

    def chat_log_and_send():
        reset(); page.locator(".chat-row").first.click(); log=page.locator("#messages")
        assert_true(log.get_attribute("role") == "log", "conversation must expose log semantics")
        assert_true(log.get_attribute("aria-live") == "polite", "conversation log must be polite")
        before=log.locator(".message").count(); comp=page.locator("#composerInput"); comp.fill("new fixture message"); page.locator("#sendBtn").click()
        assert_true(log.locator(".message").count() == before+1, "send must append message")
        assert_true("new fixture message" in log.locator(".message").last.inner_text(), "new message text missing")
        assert_true(page.evaluate("document.activeElement.id") == "composerInput", "sending should restore composer focus")
    check("chat_log_dynamic_addition_and_send_focus", chat_log_and_send)

    def focus_visible():
        reset(); inp=page.locator("#rootOmniInput"); inp.focus();
        st=inp.evaluate("e=>{const s=getComputedStyle(e);return {style:s.outlineStyle,width:s.outlineWidth,color:s.outlineColor}}")
        assert_true(st["style"] != "none" and px_number(st["width"]) >= 3, f"expected >=3px visible focus outline, got {st}")
    check("visible_keyboard_focus", focus_visible)

    def target_sizes():
        reset()
        for sel in ["#settingsBtn","#rootOmniTrigger","#omnibarRoot .voice-button"]:
            loc=page.locator(sel).first; box=loc.bounding_box()
            assert_true(box is not None, f"{sel} not measurable")
            assert_true(box["width"] >= 44 and box["height"] >= 44, f"{sel} is {box['width']:.1f}x{box['height']:.1f}; OM normal minimum is 44x44")
        page.locator(".chat-row").first.click()
        for sel in ["#conversationOmniTrigger","#sendBtn",".attach-button",".retry-button"]:
            loc=page.locator(sel).first; box=loc.bounding_box()
            assert_true(box is not None, f"{sel} not measurable")
            assert_true(box["width"] >= 44 and box["height"] >= 44, f"{sel} is {box['width']:.1f}x{box['height']:.1f}; OM normal minimum is 44x44")
    check("high_frequency_targets_at_least_44px", target_sizes)

    def lower_reach_account_control():
        reset(); box=page.locator("#settingsBtn").bounding_box(); vp=page.evaluate("()=>({w:innerWidth,h:innerHeight})")
        assert_true(box is not None, "account control not measurable")
        assert_true(box["y"] + box["height"] > vp["h"] * 0.85, f"account control should occupy lower reach zone: {box}, {vp}")
        assert_true(box["x"] + box["width"] > vp["w"] - 20, f"account control should be trailing-edge by default: {box}, {vp}")
        page.locator(".chat-row").first.click()
        assert_true(page.locator("#rootShellRow").evaluate("e=>e.classList.contains('hidden')"), "root account/Omnibar row should yield to composer in conversation")
    check("default_account_control_in_lower_trailing_reach_zone", lower_reach_account_control)

    def text_200_reflow():
        reset(); page.set_viewport_size({"width":320,"height":800}); page.locator("#settingsBtn").click(); page.locator("#textScaleSelect").select_option("200"); page.keyboard.press("Escape")
        metrics=page.evaluate("()=>({doc:document.documentElement.scrollWidth,body:document.body.scrollWidth,w:innerWidth})")
        assert_true(metrics["doc"] <= metrics["w"]+1 and metrics["body"] <= metrics["w"]+1, f"root horizontal overflow at 200%: {metrics}")
        page.locator(".chat-row").first.click(); page.wait_for_timeout(20)
        metrics2=page.evaluate("()=>({doc:document.documentElement.scrollWidth,body:document.body.scrollWidth,w:innerWidth})")
        assert_true(metrics2["doc"] <= metrics2["w"]+1 and metrics2["body"] <= metrics2["w"]+1, f"conversation horizontal overflow at 200%: {metrics2}")
    check("text_200_percent_no_page_horizontal_scroll_320px", text_200_reflow)

    def reduced_motion():
        reset(); page.locator("#settingsBtn").click(); page.locator("#personalitySelect").select_option("playful"); page.locator("#motionSelect").select_option("reduce"); page.keyboard.press("Escape"); page.locator(".chat-row").first.click()
        val=page.locator(".message.outgoing").first.evaluate("e=>getComputedStyle(e).animationDuration")
        # Browser usually serializes to seconds.
        secs=float(val.rstrip('s') or 0)
        assert_true(secs <= .002, f"reduced motion should suppress expressive animation, got {val}")
    check("reduced_motion_suppresses_expressive_animation", reduced_motion)

    def pointer_equivalence():
        reset(); row=page.locator(".chat-row").first; row.click(); assert_true(page.locator("#screenTitle").inner_text()=="Maria Santos","pointer click must open conversation")
    check("pointer_path_reaches_open_conversation", pointer_equivalence)

    check("no_uncaught_page_errors", lambda: assert_true(not errors, f"page errors: {errors}"))

    context.close()

    # Touch-equivalence context: same semantic action without a gesture path.
    touch = browser.new_context(viewport={"width":390,"height":844}, has_touch=True, is_mobile=True)
    tp = touch.new_page(); tp.set_default_timeout(2500); terr=[]; tp.on("pageerror",lambda e: terr.append(str(e))); tp.set_content(html,wait_until="load")
    try:
        tp.locator(".chat-row").first.tap()
        touch_ok = tp.locator("#screenTitle").inner_text() == "Maria Santos"
        checks.append({"name":"touch_single_pointer_path_reaches_open_conversation","status":"pass" if touch_ok else "fail", **({} if touch_ok else {"detail":"tap did not open conversation"})})
    except Exception as e:
        checks.append({"name":"touch_single_pointer_path_reaches_open_conversation","status":"fail","detail":str(e)})
    if terr:
        checks.append({"name":"touch_context_no_uncaught_page_errors","status":"fail","detail":str(terr)})
    else:
        checks.append({"name":"touch_context_no_uncaught_page_errors","status":"pass"})
    touch.close(); browser.close()
    return checks


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--json", type=Path)
    ap.add_argument("--prototype", default=DEFAULT_PROTOTYPE)
    args=ap.parse_args()
    chromium_path = shutil.which("chromium") or shutil.which("google-chrome") or shutil.which("google-chrome-stable")
    chromium_version = None
    if chromium_path:
        try:
            chromium_version = subprocess.check_output([chromium_path,"--version"], text=True, timeout=3).strip()
        except Exception:
            pass
    result={"generated_at":datetime.now(timezone.utc).isoformat(),"prototype":args.prototype,"engine_versions":{"chromium":chromium_version,"firefox":None,"webkit":None},"engines":{},"scope_notes":[
        "Browser automation verifies DOM behavior and layout only; it is not VoiceOver/TalkBack/NVDA interoperability evidence.",
        "Touch test uses browser touch emulation and a single-pointer tap; physical-device ergonomics remain a separate gate.",
        "200% fixture uses OM's explicit text-scale control at a 320 CSS-pixel viewport; browser zoom must also be checked manually on supported targets."
    ]}
    html = prototype_html(args.prototype)
    with sync_playwright() as p:
        for engine in ("chromium","firefox","webkit"):
            try:
                result["engines"][engine]=test_engine(getattr(p,engine),html,engine)
            except Exception as e:
                result["engines"][engine]=[{"name":"engine_launch_or_harness","status":"fail","detail":str(e)}]
    total=passed=failed=not_run=0
    for checks in result["engines"].values():
        for c in checks:
            if c["status"] == "not_run":
                not_run += 1
                continue
            total += 1
            passed += c["status"] == "pass"
            failed += c["status"] == "fail"
    result["summary"]={"passed":passed,"total_run":total,"failed":failed,"not_run":not_run}
    if args.json:
        args.json.parent.mkdir(parents=True,exist_ok=True); args.json.write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result,indent=2))
    raise SystemExit(0 if failed==0 and total>0 else 1)

if __name__ == "__main__":
    main()
