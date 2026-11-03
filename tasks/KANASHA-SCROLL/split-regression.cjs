// Optional browser regression: install playwright-core separately and expose it
// through NODE_PATH. CHROMIUM_EXECUTABLE may point to an existing Chromium.
const { chromium } = require('playwright-core');
const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../..');
const esbuild = require(path.join(root, 'node_modules/esbuild'));
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'kanasha-split-'));

(async () => {
  await esbuild.build({
    stdin: { resolveDir: root, loader: 'tsx', contents: `
      import React from 'react'; import {createRoot} from 'react-dom/client'; import App from './src/App';
      window.received=[]; window.handlers={}; window.instances=[]; window.disposed=[];
      const workspace={id:'room',name:'Sala',panels:[{id:'original',title:'Original',profileId:'shell'}],layout:{kind:'panel',panelId:'original'}};
      const snapshot={areas:[{id:'area',name:'Empresa',rootPath:'/tmp',workspaces:[workspace]}],profiles:[{id:'shell',name:'Shell',available:true,configured:true,builtIn:true}],activeTerminalIds:['original']};
      let sequence=0;
      const replace=(node,id,leaf)=>node.kind==='panel'?(node.panelId===id?leaf:node):{...node,first:replace(node.first,id,leaf),second:replace(node.second,id,leaf)};
      const remove=(node,id)=>{if(node.kind==='panel')return node.panelId===id?null:node;const first=remove(node.first,id),second=remove(node.second,id);return first&&second?{...node,first,second}:first||second};
      window.invoke=async(command,args)=>{
        window.received.push({command,args});
        if(command==='create_terminal'){
          const id='new-'+(++sequence),p={id,title:'Novo '+sequence,profileId:'shell'};workspace.panels.push(p);
          workspace.layout=replace(workspace.layout,args.input.targetPanelId,{kind:'split',id:'split-'+sequence,ratio:.5,direction:args.input.direction,first:{kind:'panel',panelId:args.input.targetPanelId},second:{kind:'panel',panelId:id}});
        }else if(command==='start_terminal')snapshot.activeTerminalIds.push(args.terminalId);
        else if(command==='stop_terminal')snapshot.activeTerminalIds=snapshot.activeTerminalIds.filter(id=>id!==args.terminalId);
        else if(command==='remove_terminal'){workspace.panels=workspace.panels.filter(p=>p.id!==args.terminalId);workspace.layout=remove(workspace.layout,args.terminalId)}
        else if(command==='set_split_ratio'){
          const update=node=>{if(node.kind==='split'){if(node.id===args.input.splitId)node.ratio=args.input.ratio;update(node.first);update(node.second)}};update(workspace.layout);
        }else if(!['get_snapshot','prepare_area','write_terminal','resize_terminal'].includes(command))throw Error('Unexpected command: '+command);
        return structuredClone(snapshot);
      };
      window.output=data=>(window.handlers['terminal-output']||[]).forEach(handler=>handler({payload:{terminalId:'original',data}}));
      createRoot(document.querySelector('#app')).render(<App/>);
    ` },
    bundle: true, jsx: 'automatic', format: 'iife', outfile: path.join(temporary, 'app.js'), loader: { '.css': 'empty' },
    plugins: [{ name: 'transport-and-instance-probe', setup(build) {
      build.onResolve({ filter: /^@tauri-apps\// }, args => ({ path: args.path, namespace: 'tauri' }));
      build.onLoad({ filter: /.*/, namespace: 'tauri' }, () => ({ loader: 'js', contents: `
        export const invoke=(command,args)=>window.invoke(command,args);
        export const listen=async(name,handler)=>{const list=window.handlers[name]??=[];list.push(handler);return ()=>list.splice(list.indexOf(handler),1)};
        export const getCurrentWindow=()=>({onCloseRequested:async()=>()=>{},close:async()=>{}});
        export const openUrl=async()=>{};export const open=async()=>null;
      ` }));
      build.onResolve({ filter: /^@xterm\/xterm$/ }, () => ({ path: 'xterm', namespace: 'probe' }));
      build.onLoad({ filter: /.*/, namespace: 'probe' }, () => ({ resolveDir: root, loader: 'js', contents: `
        import {Terminal as Base} from ${JSON.stringify(path.join(root, 'node_modules/@xterm/xterm/lib/xterm.js'))};
        export class Terminal extends Base {constructor(options){super(options);window.instances.push(this)}dispose(){window.disposed.push(this);super.dispose()}}
      ` }));
    } }],
  });
  const browser = await chromium.launch({ headless: true, ...(process.env.CHROMIUM_EXECUTABLE ? { executablePath: process.env.CHROMIUM_EXECUTABLE } : {}) });
  try {
    const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
    const errors = [];
    page.on('pageerror', e => errors.push(String(e)));
    page.on('dialog', dialog => dialog.accept());
    await page.setContent('<div id="app"></div>');
    await page.addStyleTag({ path: path.join(root, 'src/App.css') });
    await page.addStyleTag({ path: path.join(root, 'node_modules/@xterm/xterm/css/xterm.css') });
    await page.addScriptTag({ path: path.join(temporary, 'app.js') });
    await page.waitForSelector('.xterm-screen');
    await page.evaluate(() => new Promise(resolve => {
      window.original = instances[0];
      output('Historico original\r\n\x1b[?1049h\x1b[?1006h\x1b[?1003hConversa do Codex');
      original.write('', resolve);
    }));
    const check = async label => {
      await page.waitForTimeout(80);
      const result = await page.evaluate(() => {
        const panel = document.querySelector('[aria-label="Terminal Original"]');
        const screen = panel.querySelector('.xterm-screen'), rect = screen.getBoundingClientRect();
        const count = () => received.filter(x => x.command==='write_terminal' && x.args.terminalId==='original' && x.args.input.startsWith('\x1b[<')).length;
        const before = count();
        for(let i=0;i<20;i++) screen.dispatchEvent(new WheelEvent('wheel',{bubbles:true,cancelable:true,deltaY:-8,clientX:rect.left+40,clientY:rect.top+40}));
        return { sameElement: original.element === panel.querySelector('.xterm'), disposed: disposed.includes(original), buffer: original.buffer.active.type, history: original.buffer.normal.getLine(0).translateToString(true), mouseEvents: count()-before };
      });
      console.log(label, result);
      assert.equal(result.sameElement, true); assert.equal(result.disposed, false);
      assert.equal(result.buffer, 'alternate'); assert.equal(result.history, 'Historico original'); assert(result.mouseEvents > 0);
    };
    const geometry = async () => {
      const rects = await page.locator('.terminal-panel:visible').evaluateAll(elements => elements.map(el => {const r=el.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}));
      for(const rect of rects) assert(rect.width>0 && rect.height>0);
      for(let a=0;a<rects.length;a++)for(let b=a+1;b<rects.length;b++){
        const x=Math.min(rects[a].right,rects[b].right)-Math.max(rects[a].left,rects[b].left);
        const y=Math.min(rects[a].bottom,rects[b].bottom)-Math.max(rects[a].top,rects[b].top);
        assert(x<=1 || y<=1, 'Panels overlap');
      }
      return rects;
    };
    await check('initial');
    await page.getByRole('button',{name:'Dividir Original à direita',exact:true}).click();
    await page.getByRole('region',{name:'Terminal Novo 1',exact:true}).waitFor();
    await check('split right'); await geometry();
    await page.getByRole('button',{name:'Dividir Original abaixo',exact:true}).click();
    await page.getByRole('region',{name:'Terminal Novo 2',exact:true}).waitFor();
    await check('nested split below'); await geometry();
    const divider=page.getByRole('separator',{name:'Ajustar divisão vertical',exact:true});
    await divider.press('ArrowRight');
    const box=await divider.boundingBox();
    await page.mouse.move(box.x+box.width/2,box.y+box.height/2); await page.mouse.down();
    await page.mouse.move(box.x+box.width/2+80,box.y+box.height/2); await page.mouse.up();
    await check('resize dividers'); await geometry();
    await page.getByRole('button',{name:'Maximizar Original',exact:true}).click();
    assert.equal((await geometry()).length,1); await check('maximize');
    await page.getByRole('button',{name:'Restaurar layout de Original',exact:true}).click();
    assert.equal((await geometry()).length,3); await check('restore');
    for(const name of ['Novo 1','Novo 2']){
      await page.getByRole('button',{name:'Fechar '+name,exact:true}).click();
      await page.getByRole('region',{name:'Terminal '+name,exact:true}).waitFor({state:'detached'});
      await check('close '+name); await geometry();
    }
    assert.deepEqual(errors,[]);
    console.log('PASS: actual App + TerminalPanel + xterm, mocked Tauri transport');
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode=1; }).finally(() => fs.rmSync(temporary,{recursive:true,force:true}));
