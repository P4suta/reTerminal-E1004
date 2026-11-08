use chromiumoxide::{
    browser::{Browser, BrowserConfig},
    cdp::browser_protocol::{
        browser::{
            DownloadProgressState, EventDownloadProgress, EventDownloadWillBegin,
            SetDownloadBehaviorBehavior, SetDownloadBehaviorParams,
        },
        emulation::SetDeviceMetricsOverrideParams,
        page::CaptureScreenshotFormat,
    },
    page::ScreenshotParams,
    Page,
};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::{BufRead, BufReader},
    net::SocketAddr,
    process::{Child, Command, Stdio},
    time::Duration,
};

type BrowserResult = Result<(), Box<dyn Error + Send + Sync>>;
const DESKTOP_WIDTH: u32 = 1280;
const DESKTOP_HEIGHT: u32 = 900;
const MOBILE_WIDTH: u32 = 390;
const MOBILE_HEIGHT: u32 = 844;
const INITIALIZATION_POLLS: usize = 100;
const INITIALIZATION_POLL_MS: u64 = 20;
const ORIENTATION_COUNT: usize = 4;
const SCREEN_COUNT: usize = 7;
struct Server(Child);
impl Server {
    async fn start() -> Result<(Self, String), Box<dyn Error + Send + Sync>> {
        let mut server = Self(
            Command::new(std::env::current_exe()?)
                .args(["serve", "0"])
                .stdout(Stdio::piped())
                .spawn()?,
        );
        let stdout = server
            .0
            .stdout
            .take()
            .ok_or("missing preview server output")?;
        let line = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::task::spawn_blocking(move || {
                let mut line = String::new();
                BufReader::new(stdout).read_line(&mut line)?;
                Ok::<_, std::io::Error>(line)
            }),
        )
        .await???;
        let address: SocketAddr = line
            .trim()
            .strip_prefix("Preview: http://")
            .ok_or("preview server did not announce its listening address")?
            .parse()?;
        Ok((server, format!("http://{address}")))
    }
}

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
        let context = page.evaluate("JSON.stringify({error:document.querySelector('#error')?.textContent,status:document.querySelector('#update-status')?.textContent,focus:document.activeElement?.id,width:innerWidth,scrollWidth:document.documentElement.scrollWidth,overflow:[...document.querySelectorAll('main *')].filter(e=>e.getBoundingClientRect().right>innerWidth+1).slice(0,8).map(e=>[e.id||e.tagName,e.getBoundingClientRect().width]),state:document.querySelector('#state')?.textContent.slice(0,300)})").await;
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

async fn wait_js(page: &Page, expression: &str) -> BrowserResult {
    for _ in 0..1000 {
        if page.evaluate(expression).await?.value() == Some(&serde_json::Value::Bool(true)) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_js(page, expression).await
}

async fn media_checks(browser: &Browser, preview_url: &str) -> BrowserResult {
    let page = browser.new_page(format!("{preview_url}/?tools")).await?;
    page.execute(SetDeviceMetricsOverrideParams::new(1280, 1700, 1.0, false))
        .await?;
    assert_js(&page,"(async()=>{const r=await fetch('/pkg/NOTICE.txt');return r.ok&&r.headers.get('content-type').startsWith('text/plain')&&(await r.text()).includes('Copyright (c) Project Nayuki. (MIT License)');})()").await?;
    wait_js(
        &page,
        "document.querySelector('#state')?.textContent.length>0",
    )
    .await?;
    click(&page, "#tab-images").await?;
    page.evaluate("document.querySelector('#image-dither').value='none'; (async()=>{ const c=document.createElement('canvas'); c.width=2;c.height=2;const x=c.getContext('2d');x.fillStyle='red';x.fillRect(0,0,2,2);const blob=await new Promise(r=>c.toBlob(r));const files=new DataTransfer();files.items.add(new File([blob],'red.png',{type:'image/png'}));const e=document.querySelector('#image-file');e.files=files.files;e.dispatchEvent(new Event('change')); })()").await?;
    wait_js(
        &page,
        "document.querySelector('#media-status').textContent==='Preview updated.' && !document.querySelector('#export-image').disabled",
    )
    .await?;
    assert_js(&page,"(()=>{const x=document.querySelector('#display').getContext('2d');return [...x.getImageData(600,800,1,1).data].join()==='255,0,0,255' && [...x.getImageData(0,0,1,1).data].join()==='255,255,255,255';})()").await?;
    show_control(&page, "#image-brightness").await?;
    page.evaluate("window.mediaBeforeInvalid=document.querySelector('#display').toDataURL();document.querySelector('#image-brightness').focus();document.querySelector('#image-brightness').value='101';document.querySelector('#image-brightness').dispatchEvent(new Event('input',{bubbles:true}))").await?;
    wait_js(
        &page,
        "document.querySelector('#media-error').textContent.length>0",
    )
    .await?;
    assert_js(&page,"document.activeElement.id==='image-brightness' && document.querySelector('#image-brightness').getAttribute('aria-invalid')==='true' && document.querySelector('#display').toDataURL()===window.mediaBeforeInvalid").await?;
    page.evaluate("document.querySelector('#image-brightness').value='0';document.querySelector('#image-brightness').dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('#image-palette').value='mono';document.querySelector('#image-palette').dispatchEvent(new Event('change',{bubbles:true}))").await?;
    wait_js(
        &page,
        "document.querySelector('#display').getContext('2d').getImageData(600,800,1,1).data[0]===0",
    )
    .await?;
    page.evaluate("window.savedImage=document.querySelector('#display').toDataURL();{const files=new DataTransfer();files.items.add(new File(['<svg/>'],'external.svg',{type:'image/svg+xml'}));const e=document.querySelector('#image-file');e.files=files.files;e.dispatchEvent(new Event('change'));}").await?;
    wait_js(
        &page,
        "document.querySelector('#media-error').textContent.includes('Choose PNG')",
    )
    .await?;
    assert_js(
        &page,
        "document.querySelector('#display').toDataURL()===window.savedImage",
    )
    .await?;
    page.evaluate("document.querySelector('#poster-text').value='Library return\\nRoom 204'")
        .await?;
    click(&page, "#make-text").await?;
    wait_js(&page,"!document.querySelector('#media-error').textContent && document.querySelector('#display').toDataURL()!==window.savedImage").await?;
    page.evaluate("document.querySelector('#poster-text').value='https://example.com/'")
        .await?;
    click(&page, "#make-qr").await?;
    wait_js(&page, "!document.querySelector('#media-error').textContent").await?;
    assert_js(&page,"(()=>{const p=document.querySelector('#display').getContext('2d').getImageData(0,0,1200,1600).data;let black=0;for(let i=0;i<p.length;i+=4){if(p[i]===0)black++;if(p[i]!==p[i+1]||p[i]!==p[i+2])return false;}return black>100000&&black<900000&&p[0]===255;})()").await?;
    page.evaluate("document.querySelector('#image-palette').value='six'")
        .await?;
    click(&page, "#make-palette").await?;
    wait_js(&page, "!document.querySelector('#media-error').textContent").await?;
    assert_js(&page,"(()=>{const x=document.querySelector('#display').getContext('2d');const colors=[[0,0,0],[255,255,255],[255,255,0],[255,0,0],[0,0,255],[0,255,0]];return colors.every((rgb,i)=>[...x.getImageData(600,Math.floor(i*1600/6+100),1,1).data].slice(0,3).join()===rgb.join());})()").await?;
    page.evaluate(r#"window.serialCommands=[];window.serialReceived=0;window.serialCommitted=0;window.serialFail=false;window.serialCancel=false;window.serialClosed=false;
        const decoder=new TextDecoder(),encoder=new TextEncoder();let input;const readable=new ReadableStream({start(c){input=c;}});
        let transfer=null;window.serialInput=input;
        const writable=new WritableStream({async write(bytes){const line=decoder.decode(bytes);if(line.length>257||!line.endsWith('\n'))throw new Error('invalid envelope');const fields=line.trim().split(' ');const id=Number(fields[1]);const body=fields.slice(2);window.serialCommands.push(body.join(' '));let ok=true;
          if(body[0]==='status')input.enqueue(encoder.encode('event=shown screen=Gallery image=1 generation=1\r\nevent=status screen=Gallery refresh=Idle\r\n'));
          if(body[0]==='task'||body[0]==='note')window.deviceDaily.command(body.join(' '));
          if(body[0]==='calendar')window.deviceDaily.command(body.join(' '));
          if(body[0]==='rtc')window.deviceDaily.command('clock '+body[1]);
          if(body[0]==='task'&&body[1]==='set'&&body[2]==='1'&&window.mutateDaily){window.mutateDaily=false;document.querySelector('#task-edit-1').click();document.querySelector('#task-edit-text-1').value='Newer task';document.querySelector('#task-form-1').requestSubmit();}
          if(body[0]==='task'&&body[1]==='set'&&body[2]==='1'&&window.mutateNote){window.mutateNote=false;document.querySelector('#note-text').value='Newer note draft';}
          if(body[0]==='image'){
            if(body[1]==='begin'){transfer={id:Number(body[2]),crc:Number(body[5]),bytes:new Uint8Array(960000),received:0};}
            if(body[1]==='data'){const offset=Number(body[3]),hex=body[4];if(!transfer||Number(body[2])!==transfer.id||offset!==transfer.received)ok=false;else if(window.serialFail){ok=false;window.serialFail=false;}else{if(window.serialMutate){window.serialMutate=false;document.querySelector('#image-invert').checked=true;document.querySelector('#image-invert').dispatchEvent(new Event('change'));await new Promise(resolve=>setTimeout(resolve,200));}for(let i=0;i<hex.length;i+=2)transfer.bytes[offset+i/2]=parseInt(hex.slice(i,i+2),16);transfer.received+=hex.length/2;window.serialReceived=transfer.received;if(window.serialCancel&&transfer.received>=(window.serialCancelAt??256)){window.serialCancel=false;document.querySelector('#cancel-upload').click();}}}
            if(body[1]==='commit'){let crc=0xffffffff;if(!transfer||transfer.received!==960000)ok=false;else{for(const b of transfer.bytes){crc^=b;for(let n=0;n<8;n++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}ok=(~crc>>>0)===transfer.crc;if(ok)window.serialCommitted++;}}
            if(body[1]==='abort')transfer=null;
          }
          input.enqueue(encoder.encode(`reply ${id+100000} ok\r\nreply ${id} ${ok?'ok':'error'}\r\n`));
        }});
        window.mockPort={readable,writable,async open(options){if(options.baudRate!==115200)throw new Error('wrong baud');},async close(){if(readable.locked||writable.locked)throw new Error('stream lock leaked');window.serialClosed=true;}};
        Object.defineProperty(navigator,'serial',{configurable:true,value:{async requestPort(){return window.mockPort;}}});
        document.querySelector('#device-connect').disabled=false;
        "#).await?;
    page.evaluate("(async()=>{const {WebPreview}=await import('/pkg/web_preview.js');window.deviceDaily=new WebPreview();window.deviceDaily.command('task set 1 Bring books');window.deviceDaily.command('task done 1');})()").await?;
    click(&page, "#device-connect").await?;
    wait_js(
        &page,
        "document.querySelector('#connection-status').textContent==='Showing Image 1.' && document.querySelector('#connection-summary').textContent==='Display connected'",
    )
    .await?;
    page.evaluate("window.sentPreview=document.querySelector('#display').toDataURL();window.serialMutate=true").await?;
    click(&page, "#send-image").await?;
    wait_js(
        &page,
        "document.querySelector('#transfer-status').textContent.startsWith('Image sent')",
    )
    .await?;
    assert_js(&page,"window.serialCommitted===1 && window.serialReceived===960000 && !document.querySelector('#media-error').textContent && window.serialCommands.includes('screen gallery')").await?;
    assert_js(&page, "document.querySelector('#display').toDataURL()!==window.sentPreview && window.serialCommands.includes('rotation 0')").await?;
    page.evaluate("window.sentWhileEditing=document.querySelector('#transfer-status').textContent")
        .await?;
    click(&page, "#device-status").await?;
    wait_js(
        &page,
        "document.querySelector('#connection-status').textContent==='Showing Image 1.'",
    )
    .await?;
    page.evaluate(
        "window.serialInput.enqueue(new TextEncoder().encode('event=display_complete\\r\\n'))",
    )
    .await?;
    wait_js(
        &page,
        "document.querySelector('#transfer-status').textContent==='Display updated.'",
    )
    .await?;
    click(&page, "#tab-daily").await?;
    page.evaluate("window.beforeDraftSend=window.serialCommands.length;document.querySelector('#task-text').value='x'.repeat(49);document.querySelector('#note-text').value='Draft note'").await?;
    click(&page, "#send-settings").await?;
    wait_js(
        &page,
        "document.querySelector('#task-text').getAttribute('aria-invalid')==='true'",
    )
    .await?;
    assert_js(&page, "document.activeElement.id==='task-text' && window.serialCommands.length===window.beforeDraftSend && !document.querySelector('#state').textContent.includes('Note: Draft note')").await?;
    page.evaluate("document.querySelector('#task-text').value='Bring books';document.querySelector('#note-text').value='Draft note'").await?;
    click(&page, "#send-settings").await?;
    wait_js(
        &page,
        "document.querySelector('#daily-send-status').textContent.startsWith('Content sent')",
    )
    .await?;
    assert_js(&page, "window.serialCommands.includes('task set 1 Bring books') && window.serialCommands.includes('note set Draft note') && document.querySelector('#state').textContent.includes('Note: Draft note')").await?;
    assert_js(&page, "window.deviceDaily.text().includes('1 [ ] Bring books') && window.serialCommands.includes('task open 1')").await?;
    page.evaluate(
        "window.mutateDaily=true;document.querySelector('#daily-send-status').textContent=''",
    )
    .await?;
    click(&page, "#send-settings").await?;
    wait_js(
        &page,
        "document.querySelector('#daily-send-status').textContent.startsWith('Content sent')",
    )
    .await?;
    assert_js(&page, "window.deviceDaily.text().includes('1 [ ] Bring books') && document.querySelector('#task-label-1').textContent==='Newer task' && document.querySelector('#daily-send-status').textContent.includes('newer edits')").await?;
    page.evaluate(
        "window.mutateNote=true;document.querySelector('#daily-send-status').textContent=''",
    )
    .await?;
    click(&page, "#send-settings").await?;
    wait_js(
        &page,
        "document.querySelector('#daily-send-status').textContent.startsWith('Content sent')",
    )
    .await?;
    assert_js(&page, "window.deviceDaily.text().includes('1 [ ] Newer task') && window.deviceDaily.text().includes('Note: Draft note') && document.querySelector('#note-text').value==='Newer note draft' && document.querySelector('#daily-send-status').textContent.includes('newer edits')").await?;
    page.evaluate("document.querySelector('#daily-send-status').textContent=''")
        .await?;
    click(&page, "#send-settings").await?;
    wait_js(
        &page,
        "document.querySelector('#daily-send-status').textContent.startsWith('Content sent')",
    )
    .await?;
    assert_js(&page, "window.deviceDaily.text().includes('1 [ ] Newer task') && window.deviceDaily.text().includes('Note: Newer note draft') && !document.querySelector('#daily-send-status').textContent.includes('newer edits')").await?;
    click(&page, "#daily-calendar").await?;
    click(&page, "#calendar-next").await?;
    page.evaluate("document.querySelector('#daily-send-status').textContent=''")
        .await?;
    click(&page, "#send-settings").await?;
    wait_js(
        &page,
        "document.querySelector('#daily-send-status').textContent.startsWith('Content sent')",
    )
    .await?;
    assert_js(&page, "window.serialCommands.includes('calendar 2026-12') && window.serialCommands.includes('rtc 2026-11-05T19:30:00') && window.deviceDaily.text().includes('Calendar: December 2026') && window.deviceDaily.text().includes('Clock: 2026-11-05 19:30:00') && !document.querySelector('#daily-send-status').textContent.includes('newer edits')").await?;
    click(&page, "#calendar-today").await?;
    page.evaluate("document.querySelector('#daily-send-status').textContent=''")
        .await?;
    click(&page, "#send-settings").await?;
    wait_js(
        &page,
        "document.querySelector('#daily-send-status').textContent.startsWith('Content sent')",
    )
    .await?;
    assert_js(&page, "window.serialCommands.includes('calendar today') && window.deviceDaily.text().includes('Calendar: November 2026')").await?;
    click(&page, "#tab-images").await?;
    page.evaluate("window.serialCancel=true;document.querySelector('#send-image').click()")
        .await?;
    wait_js(
        &page,
        "document.querySelector('#media-error').textContent.includes('Sending cancelled')",
    )
    .await?;
    assert_js(
        &page,
        "window.serialCommitted===1 && window.serialCommands.at(-1).startsWith('image abort ')",
    )
    .await?;
    page.evaluate("document.querySelector('#media-error').textContent='';window.serialCancel=true;window.serialCancelAt=960000;document.querySelector('#send-image').click()").await?;
    wait_js(
        &page,
        "document.querySelector('#media-error').textContent.includes('Sending cancelled')",
    )
    .await?;
    assert_js(
        &page,
        "window.serialCommitted===1 && window.serialCommands.at(-1).startsWith('image abort ')",
    )
    .await?;
    assert_js(&page, "window.sentWhileEditing.includes('newer edits')").await?;
    page.evaluate("window.serialFail=true;document.querySelector('#send-image').click()")
        .await?;
    wait_js(
        &page,
        "document.querySelector('#media-error').textContent.includes('rejected')",
    )
    .await?;
    assert_js(
        &page,
        "window.serialCommitted===1 && window.serialCommands.at(-1).startsWith('image abort ')",
    )
    .await?;
    click(&page, "#device-disconnect").await?;
    wait_js(&page,"window.serialClosed && document.querySelector('#connection-status').textContent==='Display disconnected.'").await?;
    assert_js(
        &page,
        "!window.mockPort.readable.locked&&!window.mockPort.writable.locked",
    )
    .await?;
    click(&page, "#reset-image").await?;
    click(&page, "#make-palette").await?;
    click(&page, "#tab-images").await?;
    page.evaluate("window.scrollTo(0,0)").await?;
    wait_js(&page, "!document.querySelector('#media-error').textContent").await?;
    fs::write(
        "target/screenshots/color-media.png",
        page.screenshot(
            ScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Png)
                .full_page(false)
                .build(),
        )
        .await?,
    )?;
    page.close().await?;
    Ok(())
}

async fn live_image_checks(browser: &Browser, preview_url: &str) -> BrowserResult {
    let page = browser.new_page(preview_url).await?;
    page.execute(SetDeviceMetricsOverrideParams::new(1280, 900, 1.0, false))
        .await?;
    wait_js(
        &page,
        "document.querySelector('#state')?.textContent.length>0",
    )
    .await?;
    page.evaluate("document.querySelector('#image-palette').value='mono';document.querySelector('#image-dither').value='none';(async()=>{const c=document.createElement('canvas');c.width=2;c.height=2;const x=c.getContext('2d');x.fillStyle='rgb(128,128,128)';x.fillRect(0,0,2,2);const blob=await new Promise(r=>c.toBlob(r));const files=new DataTransfer();files.items.add(new File([blob],'gray.png',{type:'image/png'}));const input=document.querySelector('#image-file');input.files=files.files;input.dispatchEvent(new Event('change'));})()").await?;
    wait_js(
        &page,
        "document.querySelector('#image-name').textContent==='gray.png' && !document.querySelector('#export-image').disabled",
    )
    .await?;
    page.evaluate("window.beforeDither=document.querySelector('#display').toDataURL();document.querySelector('#image-dither').value='ordered';document.querySelector('#image-dither').dispatchEvent(new Event('change',{bubbles:true}))").await?;
    wait_js(
        &page,
        "document.querySelector('#display').toDataURL()!==window.beforeDither",
    )
    .await?;
    page.evaluate(r#"window.chooseFile=async(file)=>{const files=new DataTransfer();files.items.add(file);const input=document.querySelector('#image-file');input.files=files.files;input.dispatchEvent(new Event('change'));};
        window.fixtureFile=async(name)=>{const c=document.createElement('canvas');c.width=32;c.height=24;const x=c.getContext('2d');const image=x.createImageData(32,24);for(let y=0;y<24;y++)for(let x=0;x<32;x++){const i=(y*32+x)*4;image.data.set((x+y)%11===0?[0,0,0,0]:[(x*37+y*13)%256,(x*9+y*41)%256,(x*53+y*7)%256,255],i);}x.putImageData(image,0,0);return new File([await new Promise(r=>c.toBlob(r))],name,{type:'image/png'});};
        (async()=>chooseFile(await fixtureFile('colors.png')))();"#).await?;
    wait_js(&page, "document.querySelector('#image-name').textContent==='colors.png' && !document.querySelector('#export-image').disabled").await?;
    let base = ImageEditFixture::default();
    let controls = [
        "rotation",
        "image-fit",
        "image-anchor",
        "image-palette",
        "image-dither",
        "image-brightness",
        "image-contrast",
        "image-saturation",
        "image-invert",
    ];
    assert_js(&page, &format!("[...document.querySelectorAll('#panel-images input,#panel-images select')].map(e=>e.id).filter(id=>!['image-slot','poster-size','slideshow-seconds'].includes(id)).sort().join() === {}.sort().join()", serde_json::to_string(&controls)?)).await?;
    for field in controls {
        click(&page, "#reset-image").await?;
        let mut fixture = base;
        match field {
            "rotation" => fixture.rotation = device_core::geometry::Rotation::Landscape,
            "image-fit" => fixture.fit = device_core::image::Fit::Cover,
            "image-anchor" => fixture.anchor = device_core::image::Anchor::TopLeft,
            "image-palette" => fixture.palette = device_core::image::Palette::Monochrome,
            "image-dither" => fixture.dither = device_core::image::Dither::FloydSteinberg,
            "image-brightness" => fixture.brightness = 100,
            "image-contrast" => fixture.contrast = 0,
            "image-saturation" => fixture.saturation = 0,
            "image-invert" => fixture.invert = true,
            _ => return Err("uncovered image control".into()),
        }
        verify_image_edit(&page, fixture, field).await?;
    }
    for rotation in 0..4 {
        for fit in 0..4 {
            let fixture = ImageEditFixture {
                rotation: device_core::geometry::Rotation::from_code(rotation)
                    .ok_or("fixture rotation")?,
                fit: device_core::image::Fit::from_code(fit).ok_or("fixture fit")?,
                ..base
            };
            verify_image_edit(&page, fixture, "rotation").await?;
        }
    }
    for anchor in 0..9 {
        verify_image_edit(
            &page,
            ImageEditFixture {
                fit: device_core::image::Fit::Cover,
                anchor: device_core::image::Anchor::from_code(anchor).ok_or("fixture anchor")?,
                ..base
            },
            "image-anchor",
        )
        .await?;
    }
    for palette in [
        device_core::image::Palette::SixColor,
        device_core::image::Palette::Monochrome,
    ] {
        for dither in [
            device_core::image::Dither::None,
            device_core::image::Dither::Ordered,
            device_core::image::Dither::FloydSteinberg,
        ] {
            verify_image_edit(
                &page,
                ImageEditFixture {
                    palette,
                    dither,
                    ..base
                },
                "image-dither",
            )
            .await?;
        }
    }
    for fixture in [
        ImageEditFixture {
            brightness: -100,
            ..base
        },
        ImageEditFixture {
            contrast: 200,
            saturation: 200,
            ..base
        },
    ] {
        verify_image_edit(&page, fixture, "image-brightness").await?;
    }
    println!("Live image controls matched native pixels in 42 editing cases");
    click(&page, "#reset-image").await?;
    page.evaluate("window.validImage=document.querySelector('#display').toDataURL();document.querySelector('#image-brightness').focus();document.querySelector('#image-brightness').value='101';document.querySelector('#image-brightness').dispatchEvent(new Event('input',{bubbles:true}))").await?;
    wait_js(
        &page,
        "document.querySelector('#media-error').textContent.includes('Brightness')",
    )
    .await?;
    assert_js(&page, "document.activeElement.id==='image-brightness' && document.querySelector('#image-brightness').getAttribute('aria-invalid')==='true' && document.querySelector('#display').toDataURL()===window.validImage && document.querySelector('#export-image').disabled && document.querySelector('#send-image').disabled && document.querySelector('#display').getAttribute('aria-busy')==='false'").await?;
    page.evaluate("document.querySelector('#image-brightness').value='50';document.querySelector('#image-brightness').dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('#image-brightness').value='-100';document.querySelector('#image-brightness').dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('#image-brightness').value='0';document.querySelector('#image-brightness').dispatchEvent(new Event('input',{bubbles:true}))").await?;
    wait_js(&page, "!document.querySelector('#export-image').disabled && !document.querySelector('#media-error').textContent").await?;
    assert_js(&page, "document.querySelector('#display').toDataURL()===window.validImage && document.activeElement.id==='image-brightness'").await?;
    page.evaluate("window.createDownloadUrl=URL.createObjectURL.bind(URL);URL.createObjectURL=blob=>{window.downloadedImage=blob;return createDownloadUrl(blob);};window.nativeDownloadClick=HTMLAnchorElement.prototype.click;HTMLAnchorElement.prototype.click=function(){window.downloadName=this.download;};").await?;
    click(&page, "#export-image").await?;
    wait_js(
        &page,
        "window.downloadedImage?.type==='image/png' && window.downloadName==='IMAGE1.png'",
    )
    .await?;
    let directory = std::env::current_dir()?
        .join("target/browser-downloads")
        .join(std::process::id().to_string());
    fs::create_dir_all(directory.parent().ok_or("download directory")?)?;
    fs::create_dir(&directory)?;
    let mut download = SetDownloadBehaviorParams::new(SetDownloadBehaviorBehavior::Allow);
    download.download_path = Some(directory.to_string_lossy().into_owned());
    download.events_enabled = Some(true);
    browser.execute(download).await?;
    let mut starting = browser.event_listener::<EventDownloadWillBegin>().await?;
    let mut progress = browser.event_listener::<EventDownloadProgress>().await?;
    page.evaluate("HTMLAnchorElement.prototype.click=window.nativeDownloadClick")
        .await?;
    click(&page, "#export-image").await?;
    let started = tokio::time::timeout(Duration::from_secs(30), starting.next())
        .await?
        .ok_or("PNG download did not start")?;
    if started.suggested_filename != "IMAGE1.png" {
        return Err(format!("Unexpected download: {}", started.suggested_filename).into());
    }
    tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(event) = progress.next().await {
            if event.guid != started.guid {
                continue;
            }
            match event.state {
                DownloadProgressState::Completed => {
                    return Ok::<(), Box<dyn Error + Send + Sync>>(())
                }
                DownloadProgressState::Canceled => return Err("PNG download was cancelled".into()),
                DownloadProgressState::InProgress => {}
            }
        }
        Err("PNG download progress ended before completion".into())
    })
    .await??;
    if !directory.join("IMAGE1.png").is_file() {
        let files = fs::read_dir(&directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<Vec<_>, _>>()?;
        let state = page.evaluate("JSON.stringify({error:document.querySelector('#media-error').textContent,filename:window.downloadName,type:window.downloadedImage?.type})").await?;
        return Err(
            format!("PNG download did not finish: files={files:?}, state={state:?}").into(),
        );
    }
    verify_display_png(
        &page,
        &fs::read(directory.join("IMAGE1.png"))?,
        &image_fixture_frame(base)?,
    )
    .await?;
    browser
        .execute(SetDownloadBehaviorParams::new(
            SetDownloadBehaviorBehavior::Deny,
        ))
        .await?;
    fs::remove_dir_all(&directory)?;
    page.evaluate(
        "HTMLAnchorElement.prototype.click=function(){window.downloadName=this.download;}",
    )
    .await?;
    click(&page, "#export-prepared").await?;
    assert_js(&page, r#"(async()=>{const b=new Uint8Array(await downloadedImage.arrayBuffer());window.preparedFile=b;const view=new DataView(b.buffer);const crc=bytes=>{let c=0xffffffff;for(const b of bytes){c^=b;for(let n=0;n<8;n++)c=(c>>>1)^((c&1)?0xedb88320:0);}return ~c>>>0;};const colors={0:[0,0,0],1:[255,255,255],2:[255,255,0],3:[255,0,0],5:[0,0,255],6:[0,255,0]};const shown=document.querySelector('#display').getContext('2d').getImageData(0,0,1200,1600).data;for(let i=0;i<1920000;i++){const color=colors[(b[32+(i>>1)]>>((i%2)?0:4))&15];if(!color||color.some((c,n)=>shown[i*4+n]!==c)||shown[i*4+3]!==255)return false;}return downloadName==='IMAGE1.EPD'&&b.length===960032&&view.getUint32(20,true)===crc(b.slice(32))&&view.getUint32(24,true)===crc(b.slice(0,24));})()"#).await?;
    page.evaluate("document.querySelector('#image-slot').value='2';document.querySelector('#image-slot').dispatchEvent(new Event('change'))").await?;
    wait_js(&page, "document.querySelector('#display').hidden && document.querySelector('#export-image').disabled").await?;
    page.evaluate(
        "chooseFile(new File([preparedFile],'copy.EPD',{type:'application/octet-stream'}))",
    )
    .await?;
    wait_js(&page, "document.querySelector('#image-name').textContent==='copy.EPD' && !document.querySelector('#export-image').disabled").await?;
    assert_js(
        &page,
        "document.querySelector('#display').toDataURL()===window.validImage",
    )
    .await?;
    page.evaluate("window.beforePreparedEdit=document.querySelector('#display').toDataURL();document.querySelector('#image-invert').checked=true;document.querySelector('#image-invert').dispatchEvent(new Event('change'))").await?;
    wait_js(&page, "!document.querySelector('#export-image').disabled && document.querySelector('#display').toDataURL()!==window.beforePreparedEdit").await?;
    page.evaluate("document.querySelector('#image-slot').value='1';document.querySelector('#image-slot').dispatchEvent(new Event('change'))").await?;
    wait_js(
        &page,
        "document.querySelector('#image-name').textContent==='colors.png'",
    )
    .await?;
    assert_js(&page, "!document.querySelector('#image-invert').checked && document.querySelector('#display').toDataURL()===window.validImage").await?;
    verify_image_edit(
        &page,
        ImageEditFixture {
            brightness: 40,
            ..base
        },
        "image-brightness",
    )
    .await?;
    click(&page, "#delete-image").await?;
    wait_js(&page, "!document.querySelector('#undo-delete').hidden && document.querySelector('#display').hidden").await?;
    click(&page, "#undo-delete").await?;
    wait_js(&page, "!document.querySelector('#export-image').disabled && document.querySelector('#image-name').textContent==='colors.png'").await?;
    assert_js(
        &page,
        "document.querySelector('#image-brightness').value==='40'",
    )
    .await?;
    click(&page, "#reset-image").await?;
    page.evaluate(r#"window.decodeImage=window.createImageBitmap.bind(window);window.createImageBitmap=async file=>{const bitmap=await decodeImage(file);if(file.name==='slow.png'){window.slowBitmap=bitmap;await new Promise(resolve=>{window.releaseSlow=resolve;});}return bitmap;};(async()=>chooseFile(await fixtureFile('slow.png')))();"#).await?;
    wait_js(&page, "typeof window.releaseSlow==='function'").await?;
    page.evaluate("(async()=>chooseFile(await fixtureFile('fast.png')))()")
        .await?;
    wait_js(&page, "document.querySelector('#image-name').textContent==='fast.png' && !document.querySelector('#export-image').disabled").await?;
    page.evaluate("window.fastImage=document.querySelector('#display').toDataURL();releaseSlow()")
        .await?;
    wait_js(&page, "window.slowBitmap.width===0").await?;
    assert_js(&page, "document.querySelector('#image-name').textContent==='fast.png' && document.querySelector('#display').toDataURL()===window.fastImage").await?;
    page.evaluate(
        "delete window.releaseSlow;(async()=>chooseFile(await fixtureFile('slow.png')))()",
    )
    .await?;
    wait_js(&page, "typeof window.releaseSlow==='function'").await?;
    page.evaluate("document.querySelector('#image-palette').value='mono';document.querySelector('#image-palette').dispatchEvent(new Event('change'));releaseSlow()").await?;
    wait_js(&page, "document.querySelector('#image-name').textContent==='slow.png' && !document.querySelector('#export-image').disabled").await?;
    verify_image_pixels(
        &page,
        ImageEditFixture {
            palette: device_core::image::Palette::Monochrome,
            ..base
        },
    )
    .await?;
    page.evaluate(
        "delete window.releaseSlow;(async()=>chooseFile(await fixtureFile('slow.png')))()",
    )
    .await?;
    wait_js(&page, "typeof window.releaseSlow==='function'").await?;
    page.evaluate("document.querySelector('#image-slot').value='3';document.querySelector('#image-slot').dispatchEvent(new Event('change'))").await?;
    wait_js(&page, "document.querySelector('#display').hidden").await?;
    page.evaluate("releaseSlow()").await?;
    wait_js(&page, "window.slowBitmap.width===0").await?;
    assert_js(&page, "document.querySelector('#image-name').textContent==='' && document.querySelector('#display').hidden && document.querySelector('#export-image').disabled").await?;
    page.evaluate("(async()=>chooseFile(await fixtureFile('replacement.png')))()")
        .await?;
    wait_js(&page, "document.querySelector('#image-name').textContent==='replacement.png' && !document.querySelector('#export-image').disabled").await?;
    click(&page, "#delete-image").await?;
    page.evaluate("(async()=>chooseFile(await fixtureFile('replacement-again.png')))()")
        .await?;
    wait_js(&page, "document.querySelector('#image-name').textContent==='replacement-again.png' && !document.querySelector('#export-image').disabled").await?;
    assert_js(&page, "document.querySelector('#undo-delete').hidden").await?;
    page.evaluate("document.querySelector('#image-slot').value='1';document.querySelector('#image-slot').dispatchEvent(new Event('change'))").await?;
    wait_js(&page, "!document.querySelector('#export-image').disabled").await?;
    click(&page, "#reset-image").await?;
    page.evaluate("document.querySelectorAll('#panel-images details').forEach(e=>e.open=false);window.scrollTo(0,0)").await?;
    save_screenshot(&page, "image-editor.png").await?;
    page.execute(SetDeviceMetricsOverrideParams::new(390, 844, 1.0, false))
        .await?;
    save_screenshot(&page, "image-editor-mobile.png").await?;
    page.close().await?;
    Ok(())
}

async fn icon_failures(page: &Page) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
    Ok(page.evaluate(r#"(() => {
        const failures=[];
        for (const control of document.querySelectorAll('button,a[href],summary,input:not([type=hidden]),select,textarea,[role=button],[role=link],[tabindex]:not([role=tabpanel])')) {
            const visible=control.getClientRects().length>0;
            const field=control.matches('input,select,textarea');
            const labels=field?[...control.labels]:control.getAttribute('role')==='region'?(control.getAttribute('aria-labelledby')||'').split(' ').map(id=>document.getElementById(id)).filter(Boolean):[control];
            const name=control.getAttribute('aria-label')||labels.map(label=>label.textContent.trim()).join(' ');
            const native=control.matches('input[type=checkbox],input[type=radio]');
            const icon=labels.map(label=>label.querySelector(':scope > svg.control-icon')).find(Boolean);
            const monochrome=icon&&[icon,...icon.querySelectorAll('*')].every(part=>{const style=getComputedStyle(part);return ['none','rgb(0, 0, 0)','rgb(255, 255, 255)'].includes(style.fill)&&['none','rgb(0, 0, 0)','rgb(255, 255, 255)'].includes(style.stroke);});
            const valid=native?getComputedStyle(control).appearance==='auto':monochrome&&icon?.namespaceURI==='http://www.w3.org/2000/svg'&&icon.getAttribute('aria-hidden')==='true'&&icon.getAttribute('focusable')==='false'&&icon.querySelector('path')?.getAttribute('d')?.length>0&&(!visible||icon.getBoundingClientRect().width>=18&&icon.getBoundingClientRect().height>=18&&getComputedStyle(icon).visibility==='visible');
            if (!valid||visible&&!name) failures.push(control.id||control.tagName+':'+control.textContent.trim());
        }
        return failures;
    })()"#).await?.into_value()?)
}

async fn assert_icons(page: &Page) -> BrowserResult {
    let failures = icon_failures(page).await?;
    if !failures.is_empty() {
        return Err(format!("Controls lack a visible named icon: {failures:?}").into());
    }
    Ok(())
}

async fn icon_contract_checks(page: &Page) -> BrowserResult {
    assert_icons(page).await?;
    page.evaluate("{const button=document.createElement('button');button.id='icon-fixture';button.textContent='Delete fixture';document.body.append(button);}").await?;
    if !icon_failures(page).await?.contains(&"icon-fixture".into()) {
        return Err("icon gate accepted a button without an icon".into());
    }
    page.evaluate("document.querySelector('#icon-fixture').prepend(document.querySelector('#tab-images svg').cloneNode(true))").await?;
    assert_icons(page).await?;
    for defect in [
        "setAttribute('aria-hidden','false')",
        "style.display='none'",
        "querySelector('path').removeAttribute('d')",
        "querySelector('path').setAttribute('stroke','red')",
        "querySelector('path').setAttribute('fill','#777')",
    ] {
        page.evaluate(format!(
            "document.querySelector('#icon-fixture svg').{defect}"
        ))
        .await?;
        if !icon_failures(page).await?.contains(&"icon-fixture".into()) {
            return Err(format!("icon gate accepted {defect}").into());
        }
        page.evaluate("document.querySelector('#icon-fixture svg').remove();document.querySelector('#icon-fixture').prepend(document.querySelector('#tab-images svg').cloneNode(true))").await?;
    }
    page.evaluate("document.querySelector('#icon-fixture').remove()")
        .await?;
    assert_icons(page).await
}

async fn task_list_checks(browser: &Browser, preview_url: &str) -> BrowserResult {
    let page = browser.new_page(preview_url).await?;
    page.execute(SetDeviceMetricsOverrideParams::new(1280, 900, 1.0, false))
        .await?;
    wait_js(
        &page,
        "document.querySelector('#state')?.textContent.length>0",
    )
    .await?;
    click(&page, "#tab-daily").await?;
    assert_js(&page, "document.querySelector('#task-list')?.tagName==='UL' && !document.querySelector('#task-number') && document.querySelector('#task-empty').getClientRects().length>0").await?;
    page.evaluate("document.querySelector('#task-text').value='Buy groceries'")
        .await?;
    page.find_element("#task-text")
        .await?
        .focus()
        .await?
        .press_key("Enter")
        .await?;
    assert_js(&page, "document.querySelector('#task-label-1').textContent==='Buy groceries' && document.querySelector('#task-text').value==='' && document.activeElement.id==='task-text' && !document.querySelector('#task-done-1').checked").await?;
    page.find_element("#task-done-1")
        .await?
        .focus()
        .await?
        .press_key(" ")
        .await?;
    assert_js(&page, "document.querySelector('#task-done-1').checked && document.querySelector('#state').textContent.includes('1 [x] Buy groceries') && document.activeElement.id==='task-done-1'").await?;
    click(&page, "#task-edit-1").await?;
    page.evaluate("document.querySelector('#task-edit-text-1').value='Discard draft'")
        .await?;
    page.evaluate("document.querySelector('#task-edit-text-1').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',isComposing:true,bubbles:true}))").await?;
    assert_js(&page, "!document.querySelector('#task-form-1').hidden && document.querySelector('#task-edit-text-1').value==='Discard draft'").await?;
    page.find_element("#task-edit-text-1")
        .await?
        .press_key("Escape")
        .await?;
    assert_js(&page, "document.querySelector('#task-form-1').hidden && document.querySelector('#task-label-1').textContent==='Buy groceries' && document.activeElement.id==='task-edit-1'").await?;
    click(&page, "#task-edit-1").await?;
    page.evaluate("document.querySelector('#task-edit-text-1').value='Bring books'")
        .await?;
    page.find_element("#task-edit-text-1")
        .await?
        .press_key("Enter")
        .await?;
    assert_js(&page, "document.querySelector('#task-label-1').textContent==='Bring books' && !document.querySelector('#task-done-1').checked && document.activeElement.id==='task-edit-1'").await?;
    click(&page, "#task-delete-1").await?;
    assert_js(&page, "document.querySelector('#task-row-1').hidden && !document.querySelector('#task-undo').hidden && document.activeElement.id==='task-text'").await?;
    click(&page, "#task-undo").await?;
    assert_js(&page, "!document.querySelector('#task-row-1').hidden && document.querySelector('#task-label-1').textContent==='Bring books' && document.querySelector('#task-undo').hidden").await?;
    for number in 2..=6 {
        page.evaluate(format!("document.querySelector('#task-text').value='Task {number}';document.querySelector('#task-editor').requestSubmit()")).await?;
    }
    assert_js(&page, "document.querySelector('#task-save').disabled && !document.querySelector('#task-full').hidden && document.querySelectorAll('#task-list > li:not([hidden])').length===6").await?;
    assert_js(&page, "document.activeElement.id==='task-done-6'").await?;
    page.evaluate("window.fullList=document.querySelector('#state').textContent;document.querySelector('#task-text').value='One more';document.querySelector('#task-editor').requestSubmit()").await?;
    assert_js(&page, "document.querySelector('#state').textContent===window.fullList && document.querySelector('#task-text').value==='One more' && document.querySelector('#task-text-error').textContent.includes('full')").await?;
    page.evaluate("document.querySelector('#task-text').value='';document.querySelector('#task-done-2').click();document.querySelector('#task-done-4').click()").await?;
    click(&page, "#task-clear-completed").await?;
    assert_js(&page, "document.querySelector('#task-row-2').hidden && document.querySelector('#task-row-4').hidden && document.querySelector('#task-count').textContent==='0 of 4 completed' && !document.querySelector('#task-save').disabled").await?;
    click(&page, "#task-undo").await?;
    assert_js(&page, "document.querySelector('#task-done-2').checked && document.querySelector('#task-done-4').checked && document.querySelector('#task-label-1').textContent==='Bring books'").await?;
    click(&page, "#task-edit-3").await?;
    page.evaluate("document.querySelector('#task-edit-text-3').value='Unfinished edit';document.querySelector('#task-text').value='Unfinished addition'").await?;
    click(&page, "#daily-notes").await?;
    page.evaluate("document.querySelector('#note-text').value='Unfinished note'")
        .await?;
    click(&page, "#tab-device").await?;
    click(&page, "#tab-daily").await?;
    assert_js(&page, "document.querySelector('#daily-notes').getAttribute('aria-pressed')==='true' && document.querySelector('#note-text').value==='Unfinished note'").await?;
    click(&page, "#daily-checklist").await?;
    assert_js(&page, "!document.querySelector('#task-form-3').hidden && document.querySelector('#task-edit-text-3').value==='Unfinished edit' && document.querySelector('#task-text').value==='Unfinished addition'").await?;
    page.evaluate("document.querySelector('#task-edit-text-3').value='日本語';document.querySelector('#task-form-3').requestSubmit()").await?;
    assert_js(&page, "document.activeElement.id==='task-edit-text-3' && document.querySelector('#task-edit-text-3-error').textContent.includes('ASCII') && document.querySelector('#task-label-3').textContent==='Task 3'").await?;
    page.find_element("#task-edit-text-3")
        .await?
        .press_key("Escape")
        .await?;
    page.evaluate("document.querySelector('#task-text').value=''")
        .await?;
    click(&page, "#task-delete-1").await?;
    page.evaluate("document.querySelector('#task-text').value='Replacement';document.querySelector('#task-editor').requestSubmit()").await?;
    assert_js(&page, "document.querySelector('#task-undo').disabled && !document.querySelector('#task-undo-help').hidden && document.querySelector('#task-label-1').textContent==='Replacement'").await?;
    page.evaluate("document.querySelector('#task-undo').dispatchEvent(new Event('click'))")
        .await?;
    assert_js(&page, "document.querySelector('#task-label-1').textContent==='Replacement' && document.querySelector('#error').textContent.includes('unavailable')").await?;
    click(&page, "#task-edit-1").await?;
    page.evaluate("document.querySelector('#task-edit-text-1').value='<b>Keep plain text</b>'")
        .await?;
    page.find_element("#task-edit-text-1")
        .await?
        .press_key("Enter")
        .await?;
    assert_js(&page, "document.querySelector('#task-label-1').textContent==='<b>Keep plain text</b>' && !document.querySelector('#task-label-1 b')").await?;
    icon_contract_checks(&page).await?;
    page.evaluate("['Buy groceries','Return library books','Water the plants','Book a dentist appointment','Plan the weekend','Send the project update'].forEach((text,index)=>{const n=index+1;document.querySelector('#task-edit-'+n).click();document.querySelector('#task-edit-text-'+n).value=text;document.querySelector('#task-form-'+n).requestSubmit();});document.querySelector('#task-done-2').click();document.querySelector('#task-done-4').click();document.querySelector('#task-delete-6').click()").await?;
    page.evaluate("document.querySelector('#task-edit-1').click();document.querySelector('#task-edit-text-1').value='A'.repeat(48);document.querySelector('#task-form-1').requestSubmit()").await?;
    for width in [320, 390, 768, 1280] {
        page.execute(SetDeviceMetricsOverrideParams::new(width, 844, 1.0, false))
            .await?;
        page.evaluate("window.scrollTo(0,0)").await?;
        assert_js(&page, "document.documentElement.scrollWidth<=innerWidth && document.querySelector('#task-text').getBoundingClientRect().bottom<innerHeight && document.querySelector('#task-row-1').getBoundingClientRect().top<innerHeight").await?;
        assert_icons(&page).await?;
        page.evaluate("document.documentElement.style.fontSize='200%'")
            .await?;
        assert_js(&page, "document.documentElement.scrollWidth<=innerWidth && [...document.querySelectorAll('#task-list button')].filter(e=>e.getClientRects().length).every(e=>{const r=e.getBoundingClientRect();return r.width>=48&&r.height>=48;})").await?;
        page.evaluate("document.documentElement.style.fontSize=''")
            .await?;
    }
    page.evaluate("document.querySelector('#task-edit-1').click();document.querySelector('#task-edit-text-1').value='Buy groceries';document.querySelector('#task-form-1').requestSubmit()").await?;
    page.execute(SetDeviceMetricsOverrideParams::new(1280, 900, 1.0, false))
        .await?;
    page.evaluate("window.scrollTo(0,0)").await?;
    save_screenshot(&page, "task-list-desktop.png").await?;
    page.execute(SetDeviceMetricsOverrideParams::new(390, 844, 1.0, false))
        .await?;
    save_screenshot(&page, "task-list-mobile.png").await?;
    click(&page, "#daily-notes").await?;
    page.evaluate("document.querySelector('#note-text').value='Save this note';document.querySelector('#note-editor').requestSubmit();document.querySelector('#note-text').value='Discard this draft'").await?;
    click(&page, "#note-discard").await?;
    assert_js(&page, "document.querySelector('#note-text').value==='Save this note' && document.querySelector('#state').textContent.includes('Note: Save this note')").await?;
    save_screenshot(&page, "note-editor-mobile.png").await?;
    page.evaluate("window.RealDate=Date;window.Date=class extends RealDate{constructor(...args){super(...(args.length?args:[2028,1,29,12,0,0]));}}").await?;
    click(&page, "#tab-images").await?;
    click(&page, "#tab-daily").await?;
    click(&page, "#daily-calendar").await?;
    assert_js(&page, "document.querySelectorAll('#calendar-grid th[scope=col]').length===7 && document.querySelectorAll('#calendar-days time').length===29 && document.querySelector('#calendar-days [aria-current=date]')?.getAttribute('datetime')==='2028-02-29'").await?;
    let month = page
        .evaluate("document.querySelector('#calendar-month').textContent")
        .await?
        .into_value::<String>()?;
    click(&page, "#calendar-next").await?;
    assert_js(
        &page,
        &format!(
            "document.querySelector('#calendar-month').textContent!=={}",
            serde_json::to_string(&month)?
        ),
    )
    .await?;
    click(&page, "#calendar-today").await?;
    assert_js(
        &page,
        &format!(
            "document.querySelector('#calendar-month').textContent==={}",
            serde_json::to_string(&month)?
        ),
    )
    .await?;
    assert_icons(&page).await?;
    for width in [320, 768, 1280] {
        page.execute(SetDeviceMetricsOverrideParams::new(width, 844, 1.0, false))
            .await?;
        page.evaluate("document.documentElement.style.fontSize='200%'")
            .await?;
        assert_js(&page, "document.documentElement.scrollWidth<=innerWidth && document.querySelectorAll('#calendar-days time').length===29").await?;
        page.find_element("#calendar-grid")
            .await?
            .focus()
            .await?
            .press_key("ArrowRight")
            .await?;
        wait_js(&page, "document.querySelector('#calendar-grid').scrollWidth<=document.querySelector('#calendar-grid').clientWidth || document.querySelector('#calendar-grid').scrollLeft>0").await?;
        page.evaluate("document.documentElement.style.fontSize=''")
            .await?;
    }
    page.execute(SetDeviceMetricsOverrideParams::new(390, 844, 1.0, false))
        .await?;
    save_screenshot(&page, "calendar-editor-mobile.png").await?;
    page.close().await?;
    let unsupported = browser.new_page("about:blank").await?;
    unsupported
        .evaluate_on_new_document("delete Navigator.prototype.serial")
        .await?;
    unsupported.goto(preview_url).await?;
    wait_js(
        &unsupported,
        "document.querySelector('#state')?.textContent.length>0",
    )
    .await?;
    click(&unsupported, "#tab-daily").await?;
    assert_js(&unsupported, "document.querySelector('#send-settings').disabled && document.querySelector('#daily-send-help').textContent.includes('Chrome or Edge')").await?;
    click(&unsupported, "#tab-device").await?;
    assert_js(&unsupported, "document.querySelector('#device-connect').disabled && document.querySelector('#connection-help').textContent.includes('Chrome or Edge')").await?;
    assert_icons(&unsupported).await?;
    unsupported.close().await?;
    println!(
        "Direct task editing, deletion recovery, calendar dates, and named monochrome icons passed"
    );
    Ok(())
}

async fn workspace_checks(browser: &Browser, preview_url: &str) -> BrowserResult {
    let page = browser.new_page(preview_url).await?;
    page.execute(SetDeviceMetricsOverrideParams::new(1280, 900, 1.0, false))
        .await?;
    wait_js(
        &page,
        "document.querySelector('#state')?.textContent.length>0",
    )
    .await?;
    assert_js(&page, "document.querySelector('#tab-tools').hidden && document.querySelector('#tab-images').getAttribute('aria-selected')==='true' && document.querySelectorAll('[role=tabpanel]:not([hidden])').length===1 && document.querySelector('#panel-images').getAttribute('aria-labelledby')==='tab-images' && document.querySelector('#empty-image').getClientRects().length>0 && document.querySelector('#export-image').disabled && document.querySelector('#send-image').disabled").await?;
    for (key, selected) in [
        ("ArrowRight", "daily"),
        ("ArrowRight", "device"),
        ("ArrowRight", "images"),
        ("ArrowLeft", "device"),
        ("Home", "images"),
        ("End", "device"),
    ] {
        let active = page
            .evaluate("document.querySelector('[role=tab][aria-selected=true]').id")
            .await?
            .into_value::<String>()?;
        page.find_element(format!("#{active}"))
            .await?
            .focus()
            .await?
            .press_key(key)
            .await?;
        assert_js(&page, &format!("document.activeElement.id==='tab-{selected}' && document.querySelector('#panel-{selected}').hidden===false && [...document.querySelectorAll('[role=tab]')].filter(e=>e.tabIndex===0).length===1 && document.querySelectorAll('[role=tabpanel]:not([hidden])').length===1")).await?;
    }
    click(&page, "#tab-daily").await?;
    page.find_element("#tab-daily")
        .await?
        .focus()
        .await?
        .press_key("Tab")
        .await?;
    assert_js(&page, "document.activeElement.id==='panel-daily'").await?;
    page.find_element("#panel-daily")
        .await?
        .press_key("Tab")
        .await?;
    assert_js(&page, "document.activeElement.id==='daily-checklist'").await?;
    page.evaluate("window.RealDate=Date;window.Date=class extends RealDate{constructor(...args){super(...(args.length?args:[2028,1,29,23,45,56]));}};document.querySelector('#tab-images').click();document.querySelector('#tab-daily').click();document.querySelector('#daily-calendar').click()").await?;
    assert_js(&page, "document.querySelector('#state').textContent.includes('Clock: 2028-02-29 23:45:56') && document.querySelector('#state').textContent.includes('Calendar: February 2028')").await?;
    page.evaluate("document.querySelector('#task-text').value='Draft task';document.querySelector('#note-text').value='Draft note'").await?;
    click(&page, "#tab-device").await?;
    assert_js(&page, "document.querySelector('#preview-pane').hidden && !document.querySelector('#panel-device').hidden && ['measurements','device-command-form','device-log','events','completion-help'].every(id=>!document.getElementById(id).getClientRects().length)").await?;
    save_screenshot(&page, "device-workspace.png").await?;
    click(&page, "#tab-daily").await?;
    assert_js(&page, "document.querySelector('#task-text').value==='Draft task' && document.querySelector('#note-text').value==='Draft note'").await?;
    click(&page, "#task-save").await?;
    click(&page, "#note-save").await?;
    save_screenshot(&page, "daily-workspace.png").await?;
    for width in [320, 768, 1280] {
        page.execute(SetDeviceMetricsOverrideParams::new(width, 844, 1.0, false))
            .await?;
        for view in ["images", "daily", "device"] {
            click(&page, &format!("#tab-{view}")).await?;
            assert_icons(&page).await?;
            page.evaluate("window.scrollTo(0,0)").await?;
            assert_js(&page, "document.documentElement.scrollWidth<=innerWidth && [...document.querySelectorAll('button,input:not([type=checkbox]):not([type=radio]),select,summary,label:has(input[type=checkbox])')].filter(e=>e.getClientRects().length).every(e=>{const r=e.getBoundingClientRect();return r.width>=48&&r.height>=48;})").await?;
            if view == "images" {
                assert_js(&page, "document.querySelector('#image-file').getBoundingClientRect().bottom<=innerHeight").await?;
            }
            page.evaluate("document.documentElement.style.fontSize='200%'")
                .await?;
            assert_js(&page, "document.documentElement.scrollWidth<=innerWidth && parseFloat(getComputedStyle(document.querySelector('[role=tab]')).fontSize)>=36").await?;
            page.evaluate("document.documentElement.style.fontSize='';{const style=document.createElement('style');style.id='spacing-test';style.textContent='* {line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important;}p{margin-bottom:2em!important;}';document.head.append(style);}").await?;
            assert_js(&page, "document.documentElement.scrollWidth<=innerWidth && [...document.querySelectorAll('button')].filter(e=>e.getClientRects().length).every(e=>e.scrollWidth<=e.clientWidth+1&&e.scrollHeight<=e.clientHeight+1)").await?;
            page.evaluate("document.querySelector('#spacing-test').remove()")
                .await?;
        }
    }
    page.close().await?;
    println!("Task views, keyboard navigation, draft preservation, and responsive states passed");
    Ok(())
}

#[derive(Clone, Copy)]
struct ImageEditFixture {
    rotation: device_core::geometry::Rotation,
    fit: device_core::image::Fit,
    anchor: device_core::image::Anchor,
    palette: device_core::image::Palette,
    dither: device_core::image::Dither,
    brightness: i16,
    contrast: u16,
    saturation: u16,
    invert: bool,
}

impl Default for ImageEditFixture {
    fn default() -> Self {
        Self {
            rotation: device_core::geometry::Rotation::Portrait,
            fit: device_core::image::Fit::Contain,
            anchor: device_core::image::Anchor::Center,
            palette: device_core::image::Palette::SixColor,
            dither: device_core::image::Dither::Ordered,
            brightness: 0,
            contrast: 100,
            saturation: 100,
            invert: false,
        }
    }
}

async fn verify_image_edit(page: &Page, fixture: ImageEditFixture, field: &str) -> BrowserResult {
    show_control(page, &format!("#{field}")).await?;
    let settings = [
        ("rotation", (fixture.rotation as u8).to_string()),
        ("image-fit", (fixture.fit as u8).to_string()),
        ("image-anchor", (fixture.anchor as u8).to_string()),
        (
            "image-palette",
            if fixture.palette == device_core::image::Palette::Monochrome {
                "mono"
            } else {
                "six"
            }
            .into(),
        ),
        (
            "image-dither",
            match fixture.dither {
                device_core::image::Dither::None => "none",
                device_core::image::Dither::Ordered => "ordered",
                device_core::image::Dither::FloydSteinberg => "diffusion",
            }
            .into(),
        ),
        ("image-brightness", fixture.brightness.to_string()),
        ("image-contrast", fixture.contrast.to_string()),
        ("image-saturation", fixture.saturation.to_string()),
        ("image-invert", fixture.invert.to_string()),
    ];
    page.evaluate(format!("window.editGeneration=document.querySelector('#display').dataset.imageGeneration;for(const [id,value] of {}){{const e=document.getElementById(id);if(e.type==='checkbox')e.checked=value==='true';else e.value=value;}}{{const e=document.getElementById({});e.focus();e.dispatchEvent(new Event(e.type==='number'?'input':'change',{{bubbles:true}}));}}", serde_json::to_string(&settings)?, serde_json::to_string(field)?)).await?;
    wait_js(page, "document.querySelector('#display').dataset.imageGeneration!==window.editGeneration && !document.querySelector('#export-image').disabled").await?;
    assert_js(
        page,
        &format!(
            "document.activeElement.id==={} && !document.querySelector('#media-error').textContent",
            serde_json::to_string(field)?
        ),
    )
    .await?;
    verify_image_pixels(page, fixture).await
}

fn image_fixture_frame(fixture: ImageEditFixture) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    use device_core::{
        geometry::FRAME_BYTES,
        image::{Conversion, Placement, RgbaImage, Size},
    };
    let mut source = Vec::new();
    for y in 0..24 {
        for x in 0..32 {
            source.extend(if (x + y) % 11 == 0 {
                [0, 0, 0, 0]
            } else {
                [
                    ((x * 37 + y * 13) % 256) as u8,
                    ((x * 9 + y * 41) % 256) as u8,
                    ((x * 53 + y * 7) % 256) as u8,
                    255,
                ]
            });
        }
    }
    let size = Size::new(32, 24).ok_or("fixture size")?;
    let (width, height) = fixture.rotation.dimensions();
    let mut frame = vec![0x11; FRAME_BYTES];
    RgbaImage::new(size, &source)
        .ok_or("fixture pixels")?
        .render(
            &mut frame,
            fixture.rotation,
            Placement::new(
                size,
                Size::new(width, height).ok_or("fixture target")?,
                fixture.fit,
                fixture.anchor,
            ),
            Conversion::new(
                fixture.palette,
                fixture.dither,
                fixture.brightness,
                fixture.contrast,
                fixture.saturation,
                fixture.invert,
            )
            .ok_or("fixture conversion")?,
            &mut vec![[0; 3]; 2 * (usize::from(width) + 2)],
        )
        .map_err(|_| "fixture render")?;
    Ok(frame)
}

async fn verify_image_pixels(page: &Page, fixture: ImageEditFixture) -> BrowserResult {
    let frame = image_fixture_frame(fixture)?;
    let expected = format!("{:x}", Sha256::digest(device_preview::rgba(&frame)));
    let actual: String = page.evaluate("(async()=>{const canvas=document.querySelector('#display');const digest=await crypto.subtle.digest('SHA-256',canvas.getContext('2d').getImageData(0,0,1200,1600).data);return [...new Uint8Array(digest)].map(b=>b.toString(16).padStart(2,'0')).join('');})()").await?.into_value()?;
    if actual != expected {
        return Err(
            format!("image editing pixels differ: actual={actual}, expected={expected}").into(),
        );
    }
    Ok(())
}

async fn save_screenshot(page: &Page, name: &str) -> BrowserResult {
    fs::create_dir_all("target/screenshots")?;
    fs::write(
        format!("target/screenshots/{name}"),
        page.screenshot(
            ScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Png)
                .full_page(false)
                .build(),
        )
        .await?,
    )?;
    Ok(())
}

async fn click(page: &Page, selector: &str) -> BrowserResult {
    show_control(page, selector).await?;
    page.find_element(selector).await?.click().await?;
    Ok(())
}

async fn show_control(page: &Page, selector: &str) -> BrowserResult {
    let tab: String = page.evaluate(format!(
        "(()=>{{const control=document.querySelector({});if(control?.closest('#preview-controls'))return 'tab-tools';if(control?.closest('#image-actions,#image-source'))return 'tab-images';return control?.closest('[role=tabpanel]')?.getAttribute('aria-labelledby')??'';}})()",
        serde_json::to_string(selector)?
    )).await?.into_value()?;
    if !tab.is_empty()
        && page
            .evaluate(format!(
                "document.getElementById({}).getAttribute('aria-selected')!=='true'",
                serde_json::to_string(&tab)?
            ))
            .await?
            .value()
            == Some(&serde_json::Value::Bool(true))
    {
        page.find_element(format!("#{tab}")).await?.click().await?;
    }
    let daily: String = page.evaluate(format!("(()=>{{const control=document.querySelector({});return ['checklist','notes','calendar'].find(name=>control?.closest('#'+name+'-content'))??'';}})()", serde_json::to_string(selector)?)).await?.into_value()?;
    if !daily.is_empty()
        && page
            .evaluate(format!(
                "document.querySelector('#daily-{daily}').getAttribute('aria-pressed')!=='true'"
            ))
            .await?
            .value()
            == Some(&serde_json::Value::Bool(true))
    {
        page.find_element(format!("#daily-{daily}"))
            .await?
            .click()
            .await?;
    }
    let summaries: Vec<String> = page.evaluate(format!("(()=>{{const summaries=[];let control=document.querySelector({});while(control){{if(control.tagName==='DETAILS'&&!control.open){{const summary=control.querySelector(':scope > summary');if(!summary.id){{window.summarySequence=(window.summarySequence??0)+1;summary.id='test-summary-'+window.summarySequence;}}summaries.push(summary.id);}}control=control.parentElement;}}return summaries.reverse();}})()", serde_json::to_string(selector)?)).await?.into_value()?;
    for summary in summaries {
        page.find_element(format!("#{summary}"))
            .await?
            .click()
            .await?;
    }
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
    let (_server, preview_url) = Server::start().await?;
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
    let result = exercise(&browser, &preview_url).await;
    browser.close().await?;
    events.await??;
    result
}

async fn exercise(browser: &Browser, preview_url: &str) -> BrowserResult {
    task_list_checks(browser, preview_url).await?;
    workspace_checks(browser, preview_url).await?;
    live_image_checks(browser, preview_url).await?;
    let page = browser.new_page(format!("{preview_url}/?tools")).await?;
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
    page.evaluate("document.querySelector('#task-text').value='Buy groceries'")
        .await?;
    show_control(&page, "#task-text").await?;
    page.find_element("#task-text")
        .await?
        .focus()
        .await?
        .press_key("Enter")
        .await?;
    assert_js(&page, "document.querySelector('#state').textContent.includes('1 [ ] Buy groceries') && document.querySelector('#daily-feedback').textContent==='Task added.' && !document.querySelector('#task-row-1').hidden").await?;
    page.evaluate("document.querySelector('#screen').value='checklist'; document.querySelector('#screen').dispatchEvent(new Event('change'))").await?;
    click(&page, "#refresh").await?;
    assert_js(&page, "document.querySelector('#state').textContent.includes('1 [x] Buy groceries') && document.querySelector('#refresh').textContent==='Reopen task' && document.querySelector('#task-done-1').checked").await?;
    click(&page, "#hold-next").await?;
    assert_js(&page, "document.querySelector('#state').textContent.includes('Selected task: 2') && document.querySelector('#refresh').textContent==='Refresh'").await?;
    show_control(&page, "#note-text").await?;
    for note in ["clear", "set alarm", "set clear"] {
        page.evaluate(format!(
            "document.querySelector('#note-text').value={}; document.querySelector('#note-editor').requestSubmit()",
            serde_json::to_string(note)?
        ))
        .await?;
        assert_js(&page, &format!(
            "document.querySelector('#note-text').value==={} && document.querySelector('#state').textContent.includes({})",
            serde_json::to_string(note)?,
            serde_json::to_string(&format!("Note: {note}"))?
        ))
        .await?;
    }
    page.evaluate("document.querySelector('#note-text').value='Return library books.'; document.querySelector('#note-editor').requestSubmit()").await?;
    assert_js(&page, "document.querySelector('#state').textContent.includes('Note: Return library books.') && document.querySelector('#daily-feedback').textContent==='Note updated.'").await?;
    page.evaluate("document.querySelector('#events').closest('details').open=true")
        .await?;
    click(&page, "#record").await?;
    show_control(&page, "#task-text").await?;
    click(&page, "#record").await?;
    show_control(&page, "#task-text").await?;
    page.evaluate("window.dailyFrame=document.querySelector('#display').toDataURL(); window.dailyRecording=document.querySelector('#events').value; document.querySelector('#task-text').value='x'.repeat(49); document.querySelector('#task-editor').requestSubmit()").await?;
    assert_js(&page, "document.activeElement.id==='task-text' && document.activeElement.getAttribute('aria-invalid')==='true' && document.querySelector('#task-text-error').textContent.includes('48') && document.querySelector('#display').toDataURL()===window.dailyFrame").await?;
    click(&page, "#record").await?;
    assert_js(
        &page,
        "document.querySelector('#events').value===window.dailyRecording",
    )
    .await?;
    show_control(&page, "#note-text").await?;
    page.evaluate("window.dailyFrame=document.querySelector('#display').toDataURL()")
        .await?;
    page.evaluate("document.querySelector('#note-text').value='日本語'; document.querySelector('#note-editor').requestSubmit()").await?;
    assert_js(&page, "document.activeElement.id==='note-text' && document.querySelector('#note-text-error').textContent.includes('ASCII') && document.querySelector('#display').toDataURL()===window.dailyFrame").await?;
    page.evaluate("document.querySelector('#screen').value='calendar'; document.querySelector('#screen').dispatchEvent(new Event('change'))").await?;
    click(&page, "#hold-next").await?;
    assert_js(&page, "document.querySelector('#state').textContent.includes('Calendar: December 2026') && document.querySelector('#refresh').textContent==='Today'").await?;
    click(&page, "#refresh").await?;
    assert_js(
        &page,
        "document.querySelector('#state').textContent.includes('Calendar: November 2026')",
    )
    .await?;
    click(&page, "#reset-example").await?;
    assert_js(&page, "!document.querySelector('[aria-invalid=true]') && document.querySelector('#task-text').value==='' && document.querySelector('#note-text').value===''").await?;
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
        "document.querySelector('#state').textContent.startsWith('Calendar')",
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
    assert_js(&page, "document.querySelector('#wait').checked && !document.querySelector('#immediate').checked && document.querySelector('#completion-help').textContent.includes('Hold middle')").await?;
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
        "document.querySelector('#state').textContent.includes('panel timeout') && document.querySelector('#display').toDataURL() === window.presentedBeforeHold && document.querySelector('#complete').disabled && document.querySelector('#update-status').textContent.includes('Hold middle to retry')",
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
    assert_js(&page, "[...document.querySelectorAll('button,input:not([type=radio]):not([type=checkbox]),select,summary,.choices label,label:has(input[type=checkbox])')].filter(e=>e.getClientRects().length).every(e=>{const r=e.getBoundingClientRect();return r.width>=44 && r.height>=44})").await?;
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
    for width in [320, 768, 1280] {
        page.execute(SetDeviceMetricsOverrideParams::new(
            width,
            i64::from(MOBILE_HEIGHT),
            1.0,
            false,
        ))
        .await?;
        assert_js(&page, "document.documentElement.scrollWidth <= innerWidth").await?;
        page.evaluate("document.documentElement.style.fontSize='200%'")
            .await?;
        assert_js(&page, "document.documentElement.scrollWidth <= innerWidth && parseFloat(getComputedStyle(document.querySelector('#task-save')).fontSize)>=36").await?;
        page.evaluate("document.querySelector('#note-text').value='Readable note.'")
            .await?;
        click(&page, "#note-save").await?;
        assert_js(
            &page,
            "document.querySelector('#state').textContent.includes('Note: Readable note.')",
        )
        .await?;
        page.evaluate("document.documentElement.style.fontSize=''; { const style=document.createElement('style'); style.id='spacing-override'; style.textContent='* { line-height:1.5!important; letter-spacing:.12em!important; word-spacing:.16em!important; } p { margin-bottom:2em!important; }'; document.head.append(style); }").await?;
        assert_js(&page, "document.documentElement.scrollWidth <= innerWidth && [...document.querySelectorAll('button')].filter(e=>e.getClientRects().length).every(e=>e.scrollHeight<=e.clientHeight+1 && e.scrollWidth<=e.clientWidth+1)").await?;
        page.evaluate("document.querySelector('#spacing-override').remove()")
            .await?;
    }
    page.execute(SetDeviceMetricsOverrideParams::new(
        320,
        i64::from(MOBILE_HEIGHT),
        1.0,
        false,
    ))
    .await?;
    page.evaluate("document.body.style.minWidth='640px'")
        .await?;
    assert_js(&page, "document.documentElement.scrollWidth>innerWidth").await?;
    page.evaluate("document.body.style.minWidth=''").await?;
    assert_js(&page, "document.documentElement.scrollWidth<=innerWidth").await?;
    media_checks(browser, preview_url).await?;
    println!("Browser daily editing, image conversion, USB control, field errors and focus, display failure recovery, labels, monochrome styles, 320-pixel reflow, 200-percent text, spacing overrides, offline resources, atomic replay, 140 native/Wasm frame comparisons, and display PNG integrity passed; sha256={hash}");
    Ok(())
}
