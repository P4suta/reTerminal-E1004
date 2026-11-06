use chromiumoxide::{
    browser::{Browser, BrowserConfig},
    cdp::browser_protocol::{
        emulation::SetDeviceMetricsOverrideParams, page::CaptureScreenshotFormat,
    },
    page::ScreenshotParams,
    Page,
};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    process::{Child, Command},
    time::Duration,
};

type BrowserResult = Result<(), Box<dyn Error + Send + Sync>>;
const PREVIEW_URL: &str = "http://127.0.0.1:8081";
const DESKTOP_WIDTH: u32 = 1280;
const DESKTOP_HEIGHT: u32 = 900;
const MOBILE_WIDTH: u32 = 390;
const MOBILE_HEIGHT: u32 = 844;
const INITIALIZATION_POLLS: usize = 100;
const INITIALIZATION_POLL_MS: u64 = 20;
const ORIENTATION_COUNT: usize = 4;
const SCREEN_COUNT: usize = 3;
struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        match self.0.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                if let Err(error) = self.0.kill() {
                    eprintln!("preview server stop failed: {error}");
                }
                if let Err(error) = self.0.wait() {
                    eprintln!("preview server wait failed: {error}");
                }
            }
            Err(error) => eprintln!("preview server status failed: {error}"),
        }
    }
}

async fn assert_js(page: &Page, expression: &str) -> BrowserResult {
    let result = page.evaluate(expression).await?;
    if result.value() != Some(&serde_json::Value::Bool(true)) {
        let context = page.evaluate("JSON.stringify({error:document.querySelector('#error')?.textContent,status:document.querySelector('#update-status')?.textContent,focus:document.activeElement?.id,state:document.querySelector('#state')?.textContent.slice(0,300)})").await;
        fs::create_dir_all("target/screenshots")?;
        match page
            .screenshot(
                ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await
        {
            Ok(bytes) => fs::write("target/screenshots/browser-failure.png", bytes)?,
            Err(error) => eprintln!("browser failure screenshot unavailable: {error}"),
        }
        return Err(format!(
            "browser assertion failed: {expression}; result={:?}; context={context:?}",
            result.value(),
        )
        .into());
    }
    Ok(())
}

async fn click(page: &Page, selector: &str) -> BrowserResult {
    page.find_element(selector).await?.click().await?;
    Ok(())
}

async fn verify_display_png(page: &Page, png: &[u8], frame: &[u8]) -> BrowserResult {
    let script = format!(
        "(async () => {{ const bitmap=await createImageBitmap(new Blob([new Uint8Array({})],{{type:'image/png'}})); const canvas=new OffscreenCanvas(bitmap.width,bitmap.height); const context=canvas.getContext('2d'); context.drawImage(bitmap,0,0); const digest=new Uint8Array(await crypto.subtle.digest('SHA-256',context.getImageData(0,0,canvas.width,canvas.height).data)); bitmap.close(); return [canvas.width,canvas.height,[...digest].map(x=>x.toString(16).padStart(2,'0')).join('')]; }})()",
        serde_json::to_string(png)?,
    );
    let actual: (u16, u16, String) = page.evaluate(script).await?.into_value()?;
    let expected = (
        device_core::geometry::WIDTH,
        device_core::geometry::HEIGHT,
        format!("{:x}", Sha256::digest(device_preview::rgba(frame))),
    );
    if actual != expected {
        return Err(format!(
            "display PNG differs from the complete native frame: {actual:?}; expected={expected:?}"
        )
        .into());
    }
    Ok(())
}

pub fn check() -> BrowserResult {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(check_async())
}

async fn check_async() -> BrowserResult {
    let _server = Server(
        Command::new(std::env::current_exe()?)
            .args(["serve", "8081"])
            .spawn()?,
    );
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .window_size(DESKTOP_WIDTH, DESKTOP_HEIGHT)
            .build()?,
    )
    .await?;
    let events = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            event?;
        }
        Ok::<(), chromiumoxide::error::CdpError>(())
    });
    let result = exercise(&browser).await;
    browser.close().await?;
    events.await??;
    result
}

async fn exercise(browser: &Browser) -> BrowserResult {
    let page = browser.new_page(PREVIEW_URL).await?;
    page.execute(SetDeviceMetricsOverrideParams::new(
        i64::from(DESKTOP_WIDTH),
        i64::from(DESKTOP_HEIGHT),
        1.0,
        false,
    ))
    .await?;
    for _ in 0..INITIALIZATION_POLLS {
        if page
            .evaluate("document.querySelector('#state')?.textContent.length > 0")
            .await?
            .value()
            == Some(&serde_json::Value::Bool(true))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(INITIALIZATION_POLL_MS)).await;
    }
    assert_js(
        &page,
        "document.querySelector('#state').textContent.length > 0",
    )
    .await?;
    assert_js(
        &page,
        "document.querySelector('h1').textContent === 'reTerminal E1004'",
    )
    .await?;
    assert_js(&page, "document.querySelector('#temperature').value === '23.45' && document.querySelector('#humidity').value === '56.78'").await?;
    assert_js(&page, "!document.querySelector('#measurements').noValidate && document.querySelector('#clock').type === 'datetime-local' && document.querySelector('#clock').min === '2000-01-01T00:00:00' && document.querySelector('#clock').max === '2099-12-31T23:59:59' && document.querySelector('#clock').step === '1'").await?;
    assert_js(&page, "['wait','immediate'].every(id => { const e=document.getElementById(id); return e.tagName==='INPUT' && e.type==='radio' && e.name==='completion' && e.closest('fieldset')?.querySelector('legend')?.textContent==='Display completion'; }) && document.querySelector('#immediate').checked && !document.querySelector('#wait').checked").await?;
    assert_js(&page, "[['temperature','-45','130','0.01'],['humidity','0','100','0.01'],['battery','2000','5000','1']].every(([id,min,max,step]) => { const e=document.getElementById(id); return e.type==='number' && e.required && e.min===min && e.max===max && e.step===step && e.validity.valid; })").await?;
    assert_js(&page, "[...document.querySelector('#measurements').elements].every(e => !e.id || !(e.id in HTMLFormElement.prototype)) && typeof document.querySelector('#measurements').reset === 'function'").await?;
    assert_js(
        &page,
        "[...document.querySelectorAll('input,select,textarea')].every(e => e.labels.length > 0)",
    )
    .await?;
    assert_js(&page, "document.querySelector('#state').textContent.includes('23.45 C') && document.querySelector('#update-status').getAttribute('role') === 'status'").await?;
    assert_js(&page, "['previous','refresh','next'].every(id => { const r=document.getElementById(id).getBoundingClientRect(); return r.top >= 0 && r.bottom <= innerHeight; })").await?;
    page.evaluate("document.querySelector('#events').closest('details').open=true")
        .await?;
    click(&page, "#apply").await?;
    click(&page, "#record").await?;
    page.evaluate("window.inputBoundaryFrame=document.querySelector('#display').toDataURL(); window.inputBoundaryRecording=document.querySelector('#events').value").await?;
    for (field, invalid) in [
        ("temperature", "-45.01"),
        ("temperature", "130.01"),
        ("humidity", "-0.01"),
        ("humidity", "100.01"),
        ("battery", "1999"),
        ("battery", "5001"),
        ("battery", "3900.5"),
        ("clock", "1999-12-31T23:59"),
        ("clock", "2100-01-01T00:00"),
    ] {
        page.evaluate(format!(
            "window.boundaryControl=document.getElementById({}); window.boundaryValue=boundaryControl.value; boundaryControl.value={}; boundaryControl.dispatchEvent(new Event('input',{{bubbles:true}})); document.querySelector('#measurements').requestSubmit()",
            serde_json::to_string(field)?, serde_json::to_string(invalid)?
        )).await?;
        assert_js(&page, "document.activeElement===window.boundaryControl && boundaryControl.getAttribute('aria-invalid')==='true' && document.querySelector('#form-error').textContent.length>0 && document.querySelector('#display').toDataURL()===window.inputBoundaryFrame").await?;
        click(&page, "#record").await?;
        assert_js(&page, "document.querySelector('#events').value===window.inputBoundaryRecording && boundaryControl.getAttribute('aria-invalid')==='true'").await?;
        page.evaluate("boundaryControl.value=window.boundaryValue; boundaryControl.dispatchEvent(new Event('input',{bubbles:true}))").await?;
        assert_js(&page, "!document.querySelector('[aria-invalid=true]') && document.querySelector('#form-error').textContent==='' && document.querySelector('#form-feedback').textContent.includes('not applied')").await?;
    }
    click(&page, "#next").await?;
    click(&page, "#record").await?;
    page.evaluate("window.beforeInvalidApply=document.querySelector('#events').value; window.beforeInvalidFrame=document.querySelector('#display').toDataURL(); document.querySelector('#clock').value='2027-11-05T19:30:00'; document.querySelector('#humidity').value='100.01'; document.querySelector('#humidity').focus(); document.querySelector('#humidity').dispatchEvent(new Event('input',{bubbles:true}))").await?;
    assert_js(&page, "document.activeElement.id==='humidity' && document.querySelector('#humidity').validity.rangeOverflow && document.querySelector('#humidity').getAttribute('aria-invalid')==='true' && document.querySelector('#humidity-error').textContent.includes('0 to 100') && document.querySelector('#form-feedback').textContent.includes('not applied') && document.querySelector('#display').toDataURL()===window.beforeInvalidFrame").await?;
    click(&page, "#apply").await?;
    assert_js(&page, "document.activeElement.id === 'humidity' && document.activeElement.getAttribute('aria-invalid') === 'true' && document.activeElement.getAttribute('aria-describedby').includes('humidity-error') && document.querySelector('#form-error').textContent.includes('Relative humidity')").await?;
    assert_js(
        &page,
        "document.querySelector('#display').toDataURL() === window.beforeInvalidFrame",
    )
    .await?;
    click(&page, "#record").await?;
    assert_js(&page, "document.querySelector('#humidity').getAttribute('aria-invalid')==='true' && document.querySelector('#humidity-error').textContent.includes('0 to 100') && document.querySelector('#form-error').textContent.includes('Relative humidity')").await?;
    assert_js(
        &page,
        "document.querySelector('#events').value === window.beforeInvalidApply",
    )
    .await?;
    page.evaluate("document.querySelector('#clock').value='2026-11-05T19:30:00'; document.querySelector('#humidity').value='56.78'").await?;
    page.find_element("#humidity")
        .await?
        .focus()
        .await?
        .press_key("Enter")
        .await?;
    assert_js(&page, "document.querySelector('#error').textContent === '' && document.querySelector('#form-error').textContent === '' && document.querySelector('#form-feedback').textContent==='Measurements applied.' && !document.querySelector('[aria-invalid=true]')").await?;
    assert_js(
        &page,
        "document.querySelector('#state').textContent.startsWith('Diagnostics')",
    )
    .await?;
    page.find_element("#previous")
        .await?
        .focus()
        .await?
        .press_key("Enter")
        .await?;
    assert_js(
        &page,
        "document.querySelector('#state').textContent.startsWith('Device Status')",
    )
    .await?;
    page.find_element("#previous")
        .await?
        .press_key("Tab")
        .await?;
    assert_js(
        &page,
        "getComputedStyle(document.activeElement).outlineStyle !== 'none'",
    )
    .await?;
    page.find_element("#immediate")
        .await?
        .focus()
        .await?
        .press_key("ArrowRight")
        .await?;
    assert_js(&page, "document.querySelector('#wait').checked && !document.querySelector('#immediate').checked && document.querySelector('#completion-help').textContent.includes('Refresh')").await?;
    page.find_element("#wait")
        .await?
        .press_key("ArrowLeft")
        .await?;
    assert_js(
        &page,
        "document.querySelector('#immediate').checked && !document.querySelector('#wait').checked",
    )
    .await?;
    click(&page, "#wait").await?;
    page.evaluate("window.presentedBeforeHold=document.querySelector('#display').toDataURL()")
        .await?;
    click(&page, "#next").await?;
    click(&page, "#wait").await?;
    assert_js(&page, "document.querySelector('#display').toDataURL() === window.presentedBeforeHold && document.querySelector('#wait').checked && !document.querySelector('#immediate').checked && !document.querySelector('#complete').disabled && document.querySelector('#completion-help').textContent.includes('An update is waiting')").await?;
    assert_js(
        &page,
        "document.querySelector('#state').textContent.includes('Updating')",
    )
    .await?;
    click(&page, "#advance").await?;
    click(&page, "#immediate").await?;
    assert_js(
        &page,
        "document.querySelector('#state').textContent.includes('panel timeout') && document.querySelector('#display').toDataURL() === window.presentedBeforeHold && document.querySelector('#complete').disabled && document.querySelector('#update-status').textContent.includes('Refresh to retry')",
    )
    .await?;
    click(&page, "#refresh").await?;
    assert_js(&page, "document.querySelector('#display').toDataURL() !== window.presentedBeforeHold && document.querySelector('#update-status').textContent.includes('Display ready')").await?;
    click(&page, "#sensor-fault").await?;
    assert_js(
        &page,
        "document.querySelector('#state').textContent.includes('bus error')",
    )
    .await?;
    click(&page, "#record").await?;
    assert_js(
        &page,
        "document.querySelector('#events').value.includes('measurements 2026-11-05T19:30 23.45 56.78')",
    )
    .await?;
    click(&page, "#replay").await?;
    assert_js(&page, "document.querySelector('#error').textContent === ''").await?;
    page.evaluate("window.beforeInvalidReplay=document.querySelector('#display').toDataURL(); document.querySelector('#events').value='next\\nunknown-command'").await?;
    click(&page, "#replay").await?;
    assert_js(&page, "document.querySelector('#error').textContent.includes('line 2') && document.querySelector('#display').toDataURL() === window.beforeInvalidReplay").await?;
    click(&page, "#reset-example").await?;
    assert_js(&page, "document.querySelector('#state').textContent.startsWith('Device Status') && document.querySelector('#state').textContent.includes('23.45 C') && document.querySelector('#rotation').value === '0' && document.querySelector('#error').textContent === ''").await?;
    assert_js(&page, "[...document.querySelectorAll('*')].every(e => { const s=getComputedStyle(e); return ['rgb(0, 0, 0)','rgb(255, 255, 255)'].includes(s.color) && ['rgb(0, 0, 0)','rgb(255, 255, 255)','rgba(0, 0, 0, 0)'].includes(s.backgroundColor) && s.opacity==='1' && s.boxShadow==='none' && s.textShadow==='none' && s.filter==='none' && s.backgroundImage==='none' && s.mixBlendMode==='normal'; })").await?;
    assert_js(&page, "performance.getEntriesByType('resource').every(e => new URL(e.name).origin === location.origin)").await?;
    assert_js(&page, "[...document.querySelectorAll('button,input:not([type=radio]),select,summary,.choices label')].filter(e=>e.getClientRects().length).every(e=>{const r=e.getBoundingClientRect();return r.width>=44 && r.height>=44})").await?;
    let fixture = fs::read_to_string("verification/fixtures/status.events")?;
    let mut native = device_preview::Preview::default();
    native.replay(&fixture)?;
    let fixture_frame = native.frame();
    let hash = format!("{:x}", Sha256::digest(&fixture_frame));
    let script = format!("(async () => {{ const m=await import('/pkg/web_preview.js'); await m.default(); const p=new m.WebPreview(); p.replay({}); const b=p.frame(); const h=new Uint8Array(await crypto.subtle.digest('SHA-256', b)); p.free(); return [...h].map(x=>x.toString(16).padStart(2,'0')).join(''); }})()", serde_json::to_string(&fixture)?);
    let wasm: String = page.evaluate(script).await?.into_value()?;
    if wasm != hash {
        return Err("native and WebAssembly frame digests differ".into());
    }
    let mut logs = Vec::new();
    let mut native_hashes = Vec::new();
    for rotation in 0..ORIENTATION_COUNT {
        for screen in 0..SCREEN_COUNT {
            for scenario in [
                "",
                "temperature -1\nhumidity 0\nbattery 2000\nclock fault",
                "temperature missing\nhumidity missing\nbattery unknown\nclock missing",
                "temperature fault\nhumidity fault\nbattery fault\nclock fault",
                "tick 900001",
            ] {
                let log = format!(
                    "{fixture}\n{scenario}\nrotation {rotation}\n{}",
                    "next\n".repeat(screen)
                );
                let mut candidate = device_preview::Preview::default();
                candidate.replay(&log)?;
                native_hashes.push(format!("{:x}", Sha256::digest(candidate.frame())));
                logs.push(log);
            }
        }
    }
    let script = format!("(async () => {{ const m=await import('/pkg/web_preview.js'); const results=[]; for(const log of {}) {{ const p=new m.WebPreview(); p.replay(log); const h=new Uint8Array(await crypto.subtle.digest('SHA-256',p.frame())); results.push([...h].map(x=>x.toString(16).padStart(2,'0')).join('')); p.free(); }} return results; }})()", serde_json::to_string(&logs)?);
    let wasm_hashes: Vec<String> = page.evaluate(script).await?.into_value()?;
    if wasm_hashes != native_hashes {
        return Err("native/Wasm screen, rotation, and measurement-state matrix differs".into());
    }
    page.evaluate(format!(
        "document.querySelector('#events').value={}",
        serde_json::to_string(&fixture)?
    ))
    .await?;
    click(&page, "#replay").await?;
    page.evaluate("window.scrollTo(0,0)").await?;
    fs::create_dir_all("target/screenshots")?;
    fs::write(
        "target/screenshots/web-desktop.png",
        page.screenshot(
            ScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Png)
                .build(),
        )
        .await?,
    )?;
    let display_png: Vec<u8> = page.evaluate("(async () => { const blob=await new Promise(resolve=>document.querySelector('#display').toBlob(resolve,'image/png')); return [...new Uint8Array(await blob.arrayBuffer())]; })()").await?.into_value()?;
    fs::write("target/screenshots/display.png", &display_png)?;
    verify_display_png(
        &page,
        &fs::read("target/screenshots/display.png")?,
        &fixture_frame,
    )
    .await?;
    for crop in [true, false] {
        let invalid_png: Vec<u8> = page.evaluate(format!("(async () => {{ const source=document.querySelector('#display'); const canvas=new OffscreenCanvas({0}?1:source.width,{0}?1:source.height); const context=canvas.getContext('2d'); context.drawImage(source,0,0); const pixel=context.getImageData(0,0,1,1); pixel.data[0]^=255; context.putImageData(pixel,0,0); return [...new Uint8Array(await (await canvas.convertToBlob({{type:'image/png'}})).arrayBuffer())]; }})()", crop)).await?.into_value()?;
        match verify_display_png(&page, &invalid_png, &fixture_frame).await {
            Err(error)
                if error
                    .to_string()
                    .starts_with("display PNG differs from the complete native frame:") => {}
            _ => return Err("display PNG control did not reject cropped or changed pixels".into()),
        }
    }
    page.execute(SetDeviceMetricsOverrideParams::new(
        i64::from(MOBILE_WIDTH),
        i64::from(MOBILE_HEIGHT),
        1.0,
        false,
    ))
    .await?;
    page.evaluate("window.scrollTo(0,0)").await?;
    assert_js(&page, "document.documentElement.scrollWidth <= innerWidth").await?;
    fs::write(
        "target/screenshots/web-mobile.png",
        page.screenshot(
            ScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Png)
                .build(),
        )
        .await?,
    )?;
    assert_js(&page, "['previous','refresh','next'].every(id => { const r=document.getElementById(id).getBoundingClientRect(); return r.top >= 0 && r.bottom <= innerHeight; })").await?;
    println!("Browser controls, keyboard submission, field errors and focus, completion failure recovery, labels, monochrome styles, narrow layout, offline resources, atomic replay, 60 native/Wasm frame comparisons, and decoded display PNG integrity with crop and pixel-change rejection passed; sha256={hash}");
    Ok(())
}
