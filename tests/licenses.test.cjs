const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto');
const full=JSON.parse(fs.readFileSync('static/licenses.json','utf8'));
function fixture(language='en',embedded=true){
 const elements=[],openButton=new Element('button');let loads=0,reply=async()=>({ok:true,json:async()=>full});
 function Element(tag){this.tagName=tag;this.children=[];this.listeners={};this.dataset={};this.attrs={};this.value='';this.open=false;this.textContent='';elements.push?.(this);}
 Element.prototype.append=function(...els){this.children.push(...els);};Element.prototype.appendChild=function(el){this.children.push(el);};
 Element.prototype.replaceChildren=function(){this.children=[];};Element.prototype.setAttribute=function(k,v){this.attrs[k]=v;};
 Element.prototype.addEventListener=function(k,v){this.listeners[k]=v;};Element.prototype.showModal=function(){this.open=true;};
 Element.prototype.close=function(){this.open=false;};Element.prototype.focus=function(){this.focused=true;};
 const body=new Element('body');
 const document={body,createElement:tag=>new Element(tag),getElementById:id=>id==='embedded-licenses'&&embedded?{textContent:JSON.stringify(full)}:null,querySelectorAll:()=>[openButton]};
 const window={__valtheumLanguage:language};
 const context=vm.createContext({document,window,AbortController,setTimeout,clearTimeout,fetch:async(...args)=>{loads++;return reply(...args);}});
 for(const file of ['i18n-catalog.js','i18n.js','licenses.js'])vm.runInContext(fs.readFileSync('static/'+file,'utf8'),context);
 return {window,elements,body,button:openButton,open:()=>window.VaultLicenses.open(),loads:()=>loads,respond:f=>reply=f,
  get dialog(){return elements.find(e=>e.tagName==='dialog');},get search(){return elements.find(e=>e.tagName==='input');},
  get list(){return elements.find(e=>e.className==='licenses-list');},get content(){return elements.find(e=>e.tagName==='pre');}};
}
test('embedded license catalog preserves every notice and contains no executable HTML',()=>{
 assert.ok(!fs.readFileSync('static/licenses.json','utf8').includes('<'));
 for(const c of full.components)for(const n of c.notices){
  const original=fs.readFileSync(n.file,'utf8');
  assert.equal(full.texts[n.text],original,n.file);
  assert.equal(crypto.createHash('sha256').update(original).digest('hex'),n.text);
 }
 assert.equal(full.components[0].license,'MIT');
 assert.ok(full.components.find(c=>c.name==='Liberation fonts').license.includes('GPL'));
 const sqlcipher=full.components.find(c=>c.name==='SQLCipher');
 assert.equal(sqlcipher.license,'BSD-3-Clause');assert.equal(sqlcipher.scope,'native');
 assert.ok(sqlcipher.notices.some(n=>n.file.includes('/sqlcipher/')));
});
for(const lang of ['ko','en','ja'])test('license window works offline, searches, preserves original texts and closes in '+lang,async()=>{
 const f=fixture(lang);await f.open();assert.equal(f.loads(),0);assert.equal(f.window.VaultLicenses.isOpen(),true);
 assert.equal(f.search.disabled,false);assert.ok(f.content.textContent.includes(fs.readFileSync('LICENSE','utf8')));
 f.search.value='Liberation';f.search.listeners.input();assert.equal(f.list.children.length,1);
 assert.ok(f.content.textContent.includes('GNU GENERAL PUBLIC LICENSE'));
 f.search.value='not-a-real-package-xyz';f.search.listeners.input();assert.equal(f.list.children.length,0);
 let prevented=false,stopped=false;f.dialog.listeners.keydown({key:'Escape',preventDefault(){prevented=true;},stopPropagation(){stopped=true;}});
 assert.ok(prevented&&stopped);assert.equal(f.window.VaultLicenses.isOpen(),false);
 await f.open();assert.equal(f.search.value,'');assert.equal(f.loads(),0);
 assert.equal(f.dialog.children[0].children[1].textContent,{ko:'닫기',en:'Close',ja:'閉じる'}[lang]);
});
test('closing a loading license dialog prevents late reopening and shares the cached response',async()=>{
 const f=fixture('en',false);let finish;
 f.respond(()=>new Promise(r=>finish=r));
 const work=f.open();assert.equal(f.search.disabled,true);f.dialog.close();
 finish({ok:true,json:async()=>full});await work;assert.equal(f.dialog.open,false);assert.ok(!f.search.focused);
 await f.open();assert.equal(f.loads(),1);assert.equal(f.dialog.open,true);
});
test('failed license download is retryable and leaves an accessible close button',async()=>{
 const f=fixture('en',false);f.respond(async()=>({ok:false}));await f.open();
 assert.equal(f.search.disabled,true);assert.equal(f.dialog.open,true);assert.equal(f.list.children.length,0);
 f.dialog.close();f.respond(async()=>({ok:true,json:async()=>full}));await f.open();
 assert.equal(f.loads(),2);assert.equal(f.search.disabled,false);assert.ok(f.content.textContent.includes('MIT License'));
});
test('all shipped UI entry points expose offline licenses',()=>{
 const main=fs.readFileSync('static/index.html','utf8'),restore=fs.readFileSync('restore-ui/index.html','utf8'),native=fs.readFileSync('src/restore_ui.rs','utf8');
 assert.equal((main.match(/data-open-licenses/g)||[]).length,2);
 assert.ok(restore.includes('data-open-licenses'));
 for(const file of ['licenses.json','licenses.css','licenses.js'])assert.ok(native.includes(file));
});

test('distribution selection excludes build/test and macro-host dependencies without losing shared runtime code',()=>{
 const {selectPackages}=require('../tools/licenses/distribution.cjs');
 const ids=['root','runtime','shared','build','dev','macro','macro-helper'];
 const dep=(pkg,kind=null)=>({pkg,dep_kinds:[{kind}]});
 const metadata={packages:ids.map(id=>({id,name:id,version:'1',source:id==='root'?null:'registry',targets:[{kind:[id==='macro'?'proc-macro':'lib']}]})),
  resolve:{root:'root',nodes:[
   {id:'root',deps:[dep('build','build'),dep('dev','dev'),dep('runtime'),dep('macro')]},
   {id:'runtime',deps:[dep('shared')]},{id:'shared',deps:[]},
   {id:'build',deps:[dep('shared'),dep('macro-helper')]},
   {id:'dev',deps:[]},{id:'macro',deps:[dep('macro-helper')]},{id:'macro-helper',deps:[]}
  ]}};
 const selected=selectPackages(metadata);
 assert.deepEqual([...selected.keys()].sort(),['macro-1','runtime-1','shared-1']);
 assert.equal(selected.get('macro-1').scope,'generated');
 assert.equal(selected.get('shared-1').scope,'app');
});

test('app catalog omits development-only components but preserves native and generated-code notices',()=>{
 const names=new Set(full.components.map(c=>c.name));
 for(const name of ['tink','cryptography','argon2-cffi','Pillow','io_tee','criterion','cc','syn','quote','embed-resource','openssl-src'])assert.ok(!names.has(name),name);
 for(const name of ['OpenSSL','libsqlite3-sys','zstd-sys','aes-gcm','PDF.js','Liberation fonts','serde_derive'])assert.ok(names.has(name),name);
 const openssl=full.components.find(c=>c.name==='OpenSSL');
 assert.equal(openssl.version,'3.6.3');assert.equal(openssl.scope,'native');
 assert.ok(openssl.notices.every(n=>n.file.endsWith('/openssl/LICENSE.txt')));
 assert.equal(full.components.find(c=>c.name==='serde_derive').scope,'generated');
 assert.ok(full.components.every(c=>c.scope!=='tools'));
 assert.ok(!full.summary.includes('## Optional Python tools'));
 assert.ok(fs.readFileSync('docs/DEVELOPMENT_LICENSES.md','utf8').includes('cryptography'));
});

test('distribution manifest covers all displayed notices and corresponding font source without development inventories',()=>{
 const path=require('node:path');
 const distribution=JSON.parse(fs.readFileSync('licenses/distribution.json','utf8'));
 assert.deepEqual(distribution.components,full.components);
 const files=new Set(distribution.files);
 for(const c of full.components)for(const n of c.notices)assert.ok(files.has(n.file),n.file);
 const source=JSON.parse(fs.readFileSync('licenses/sources/liberation-fonts-1.07.4.json','utf8')).source;
 assert.ok(files.has(source.file));
 assert.equal(crypto.createHash('sha256').update(fs.readFileSync(source.file)).digest('hex'),source.sha256);
 for(const file of files){
  assert.ok(!path.isAbsolute(file)&&!file.split(/[\\/]/).includes('..'),file);
  assert.ok(fs.statSync(file).isFile(),file);
  assert.ok(!file.startsWith('licenses/python/')&&!file.startsWith('licenses/upstream/'),file);
 }
 assert.ok(!files.has('licenses/rust/manifest.json'));assert.ok(!files.has('licenses/rust/README.md'));
 for(const match of fs.readFileSync('licenses/DISTRIBUTION.md','utf8').matchAll(/\]\(([^)]+)\)/g))
  assert.ok(fs.existsSync(path.resolve('licenses',decodeURI(match[1]))),match[1]);
});
