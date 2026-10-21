const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const idle = () => ({id:0,running:false,cancelled:false,completed:false,checked:0,total:0,ok:0,issues:0,
    unavailable:0,phase:'done',error:null,started_at:0,finished_at:null,results:[],results_truncated:false});
const running = extra => ({...idle(),id:1,running:true,total:12,phase:'files',started_at:100,...extra});
const done = extra => ({...running(),running:false,completed:true,checked:12,ok:12,phase:'done',finished_at:200,...extra});
const response = job => ({json:async()=>job});
const deferred = () => {let resolve;const promise=new Promise(done=>resolve=done);return {promise,resolve};};
async function flushUntil(predicate) {
    for(let attempt=0;attempt<30&&!predicate();attempt++)await Promise.resolve();
    assert.ok(predicate(),'the expected request was issued');
}

function fixture(language='ko') {
    class Element {
        constructor() { this.listeners={};this.children=[];this.textContent='';this.hidden=false;this.disabled=false;this.open=false; }
        addEventListener(name,callback) { this.listeners[name]=callback; }
        appendChild(child) { this.children.push(child); }
        replaceChildren(...children) { this.children=children; }
        set innerHTML(_) { throw Error('Untrusted content must not be interpreted as HTML'); }
        showModal() { this.open=true; }
        close() { this.open=false; }
    }
    const elements=new Map();
    const get=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
    const calls=[],timers=new Map();let sequence=0,generation=1,allowed=true;
    let responder=async()=>response(idle());
    const translations=JSON.parse(fs.readFileSync(path.join(__dirname,'../tools/i18n/extra.json'),'utf8'));
    const window={__valtheumLanguage:language,VaultMessages:{en:{},ja:{}}};
    for(const [source,[en,ja]] of Object.entries(translations)) {window.VaultMessages.en[source]=en;window.VaultMessages.ja[source]=ja;}
    const context=vm.createContext({window,AbortController,Date,document:{getElementById:get,createElement:()=>new Element()},
        setTimeout:(fn,ms)=>{timers.set(++sequence,{fn,ms});return sequence;},clearTimeout:id=>timers.delete(id)});
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../static/i18n.js'),'utf8'),context);
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../static/integrity-check.js'),'utf8'),context);
    const ui=new window.IntegrityCheck({fetch:async(url,options)=>{calls.push({url,options});return responder(url,options);},
        captureScope:()=>({generation}),isFresh:scope=>scope.generation===generation,canRun:()=>allowed});
    return {ui,get,calls,timers,respond:fn=>responder=fn,navigate(){generation++;},allow:value=>allowed=value,
        async poll(delay=1000){const entry=[...timers].find(([,timer])=>timer.ms===delay);assert.ok(entry,`${delay}ms poll is scheduled`);timers.delete(entry[0]);await entry[1].fn();}};
}

test('quick check opens current status and prevents duplicate starts while pending or running',async()=>{
    const f=fixture();await f.ui.open();const start=deferred();
    f.respond(async()=>start.promise);const pending=f.ui.start();await f.ui.start();
    assert.equal(f.calls.filter(call=>call.options.method==='POST').length,1);
    assert.equal(f.get('integrity-start').disabled,true);
    start.resolve(response(running()));await pending;await f.ui.start();
    assert.equal(f.calls.filter(call=>call.options.method==='POST').length,1);
    assert.equal(f.get('integrity-cancel').hidden,false);
    assert.equal(f.calls[1].options.scope.generation,1);
});

test('one-second polling updates counts, stops on completion, and retains last completion time for next run',async()=>{
    const f=fixture();f.respond(async()=>response(running({checked:2,ok:2})));await f.ui.open();
    assert.equal(f.get('integrity-progress').value,2);
    f.respond(async()=>response(done()));await f.poll();
    assert.equal(f.ui.job.completed,true);assert.equal(f.get('integrity-cancel').hidden,true);
    assert.match(f.get('integrity-summary').textContent,/12\/12/);
    const completed=f.get('integrity-finished').textContent;assert.match(completed,/마지막 완료/);
    assert.equal(f.timers.size,0);
    f.respond(async()=>response(running({id:2})));await f.ui.start();
    assert.equal(f.get('integrity-finished').textContent,completed);
});

test('closing stops polling without cancelling the server and reopening retrieves the latest result',async()=>{
    const f=fixture();f.respond(async()=>response(running()));await f.ui.open();f.ui.close();
    assert.equal(f.get('integrity-dialog').open,false);assert.equal(f.timers.size,0);
    assert.equal(f.calls.length,1);
    f.respond(async()=>response(done()));await f.ui.open();
    assert.equal(f.ui.job.completed,true);assert.equal(f.get('integrity-dialog').open,true);
    assert.deepEqual(f.calls.map(call=>call.options.method),['GET','GET']);
});

test('cancellation keeps checked results and does not claim a complete file verification',async()=>{
    const f=fixture();f.respond(async()=>response(running()));await f.ui.open();
    f.respond(async()=>response({...running(),running:false,cancelled:true,checked:3,ok:2,unavailable:1,phase:'cancelled',
        results:[{file_id:9,name:'NAS.mp4',state:'unavailable',message:'확인 불가'}]}));await f.ui.cancel();
    assert.equal(f.calls.at(-1).url,'/api/integrity/quick/cancel');
    assert.equal(f.calls.at(-1).options.method,'POST');assert.equal(f.timers.size,0);
    assert.deepEqual(JSON.parse(f.calls.at(-1).options.body),{id:1});
    assert.match(f.get('integrity-status').textContent,/취소/);
    assert.match(f.get('integrity-summary').textContent,/확인 불가 1/);
    assert.equal(f.get('integrity-results').children.length,1);
    assert.equal(f.get('integrity-start').disabled,false);
});

test('reset clears filenames, completion time, polling and ignores a delayed previous library response',async()=>{
    const f=fixture();f.respond(async()=>response(done({results:[{name:'private.vta',state:'mismatch',message:'검증 불일치'}]})));await f.ui.open();
    const delayed=deferred();f.respond(async()=>delayed.promise);const request=f.ui.open();
    const signal=f.calls.at(-1).options.signal;f.navigate();f.ui.reset();
    assert.equal(signal.aborted,true);delayed.resolve(response(done({results:[{name:'old-secret.vta',state:'missing',message:'파일 없음'}]})));await request;
    assert.equal(f.ui.job,null);assert.equal(f.get('integrity-results').children.length,0);
    assert.equal(f.get('integrity-dialog').open,false);assert.equal(f.timers.size,0);
    assert.doesNotMatch(f.get('integrity-finished').textContent,/마지막 완료/);
});

test('late status response cannot overwrite the result of a newer start request',async()=>{
    const f=fixture();await f.ui.open();const old=deferred();let first=true;
    f.respond(async()=>first?(first=false,old.promise):response(running({id:2})));
    const status=f.ui.open();await f.ui.start();old.resolve(response(idle()));await status;
    assert.equal(f.ui.job.id,2);assert.equal(f.ui.job.running,true);
});

test('filenames remain literal text and issue labels/messages are localized in English and Japanese',async()=>{
    for(const [language,label] of [['en','Unavailable'],['ja','確認不可']]) {
        const f=fixture(language);const filename='<img src=x onerror=alert(1)>확인 불가.vta';
        f.respond(async()=>response(done({ok:11,unavailable:1,results_truncated:true,
            results:[{name:filename,state:'unavailable',message:'확인 불가'}]})));await f.ui.open();
        const row=f.get('integrity-results').children[0];
        assert.equal(row.children[0].children[0].textContent,filename);
        assert.equal(row.children[0].children[1].textContent,label);assert.equal(row.children[1].textContent,label);
        assert.equal(f.get('integrity-truncated').hidden,false);
    }
});

test('request failures preserve available results and covered libraries cannot start a check',async()=>{
    const f=fixture();f.respond(async()=>response(done({results:[{name:'missing.vta',state:'missing',message:'파일 없음'}]})));await f.ui.open();
    f.respond(async()=>{throw Error('NAS unavailable');});await f.ui.start();
    assert.match(f.get('integrity-status').textContent,/NAS unavailable/);
    assert.equal(f.get('integrity-results').children.length,1);assert.equal(f.get('integrity-start').disabled,true);
    f.allow(false);const count=f.calls.length;await f.ui.open();await f.ui.start();assert.equal(f.calls.length,count);
});

test('quick check explains partial reads and omissions in all supported languages',()=>{
    const html=fs.readFileSync(path.join(__dirname,'../static/index.html'),'utf8');
    const catalog=JSON.parse(fs.readFileSync(path.join(__dirname,'../tools/i18n/extra.json'),'utf8'));
    const description='DB와 키를 확인하고, 암호화 파일의 처음·끝 일부 블록만 읽습니다. 일반 파일은 존재 여부와 크기를 확인합니다.';
    const limitation='파일 전체 해시·중간 데이터·이미지 디코딩은 검사하지 않습니다. 빠른 검사 통과가 파일 전체의 정상 상태를 보장하지는 않습니다.';
    assert.ok(html.includes(description));assert.ok(html.includes(limitation));
    for(const source of [description,limitation]) {assert.equal(catalog[source].length,2);assert.ok(catalog[source].every(value=>value&&!/[가-힣]/.test(value)));}
});

test('a lost start response immediately reconciles the running job and restores its cancel button',async()=>{
    const f=fixture();await f.ui.open();
    f.respond(async(url,options)=>{
        if(url.endsWith('/cancel'))return response({...running({id:9}),running:false,cancelled:true,phase:'cancelled'});
        if(options.method==='POST')throw new TypeError('Response lost after accepting check');
        return response(running({id:9,checked:4,ok:4}));
    });
    await f.ui.start();
    assert.deepEqual(f.calls.map(call=>call.options.method),['GET','POST','GET']);
    assert.equal(f.ui.job.id,9);assert.equal(f.ui.uncertain,false);
    assert.equal(f.get('integrity-cancel').hidden,false);assert.equal(f.get('integrity-start').disabled,true);
    await f.ui.cancel();assert.deepEqual(JSON.parse(f.calls.at(-1).options.body),{id:9});
    assert.equal(f.ui.job.cancelled,true);
});

test('failed reconciliation retries with bounded backoff without resending POST and recovers on a later status',async()=>{
    const f=fixture();await f.ui.open();f.respond(async()=>{throw new TypeError('Offline');});await f.ui.start();
    assert.equal(f.ui.uncertain,true);assert.equal(f.get('integrity-start').disabled,true);
    assert.equal(f.calls.length,3,'start failure triggers an immediate status check');
    await f.ui.start();assert.equal(f.calls.length,3,'uncertain state blocks another start');
    for(const delay of [1000,2000,4000,5000,5000])await f.poll(delay);
    assert.equal(f.calls.filter(call=>call.options.method==='POST').length,1);
    f.respond(async()=>response(done({id:8})));await f.poll(5000);
    assert.equal(f.ui.job.id,8);assert.equal(f.ui.uncertain,false);assert.equal(f.get('integrity-start').disabled,false);
    assert.equal(f.timers.size,0);
});

test('a status read from before a lost start cannot clear the uncertainty of that start',async()=>{
    const f=fixture();await f.ui.open();const previous=deferred(),reconcile=deferred();let read=0;
    f.respond(async(_url,options)=>{
        if(options.method==='POST')throw new TypeError('Lost response');
        return ++read===1?previous.promise:reconcile.promise;
    });
    const oldRead=f.ui.open(),start=f.ui.start();
    await flushUntil(()=>f.calls.length>=4);
    previous.resolve(response(idle()));await oldRead;
    assert.equal(f.ui.uncertain,true);assert.equal(f.get('integrity-start').disabled,true);
    reconcile.resolve(response(running({id:22})));await start;
    assert.equal(f.ui.job.id,22);assert.equal(f.get('integrity-cancel').hidden,false);
});

test('closing during reconciliation aborts the wait, stops retries and ignores its late result after reopening',async()=>{
    const f=fixture();await f.ui.open();const delayed=deferred();
    f.respond(async(_url,options)=>{if(options.method==='POST')throw new TypeError('Lost response');return delayed.promise;});
    const start=f.ui.start();await flushUntil(()=>f.calls.length>=3);
    const signal=f.calls.at(-1).options.signal;f.ui.close();await start;
    assert.equal(signal.aborted,true);assert.equal(f.timers.size,0);assert.equal(f.ui.uncertain,true);
    f.respond(async()=>response(running({id:12})));await f.ui.open();
    delayed.resolve(response(idle()));await Promise.resolve();await Promise.resolve();
    assert.equal(f.ui.job.id,12);assert.equal(f.get('integrity-cancel').hidden,false);
});

test('reset during recovery clears state and prevents retries or old results from exposing the previous library',async()=>{
    const f=fixture();await f.ui.open();const delayed=deferred();
    f.respond(async(_url,options)=>{if(options.method==='POST')throw new TypeError('Lost response');return delayed.promise;});
    const start=f.ui.start();await flushUntil(()=>f.calls.length>=3);
    const signal=f.calls.at(-1).options.signal;f.navigate();f.ui.reset();await start;
    delayed.resolve(response(done({results:[{name:'previous-private.vta',state:'missing',message:'파일 없음'}]})));
    await Promise.resolve();await Promise.resolve();
    assert.equal(signal.aborted,true);assert.equal(f.ui.job,null);assert.equal(f.ui.uncertain,false);
    assert.equal(f.get('integrity-results').children.length,0);assert.equal(f.timers.size,0);
});

test('a stalled start response is bounded even when abort is not forwarded, and its later response is ignored',async()=>{
    const f=fixture();await f.ui.open();const stalled=deferred();
    f.respond(async(_url,options)=>options.method==='POST'?stalled.promise:response(running({id:33})));
    const start=f.ui.start();const timeout=[...f.timers].find(([,timer])=>timer.ms===20000);
    assert.ok(timeout);f.timers.delete(timeout[0]);timeout[1].fn();await start;
    assert.equal(f.ui.job.id,33);assert.equal(f.get('integrity-cancel').hidden,false);
    stalled.resolve(response(idle()));await Promise.resolve();await Promise.resolve();assert.equal(f.ui.job.id,33);
});

test('initial status failure also retries instead of enabling a start against an unknown server state',async()=>{
    const f=fixture('en');f.respond(async()=>{throw new TypeError('Offline');});await f.ui.open();
    assert.equal(f.get('integrity-start').disabled,true);assert.doesNotMatch(f.get('integrity-status').textContent,/[가-힣]/);
    f.respond(async()=>response(idle()));await f.poll();
    assert.equal(f.get('integrity-start').disabled,false);assert.equal(f.timers.size,0);
});

test('an explicit start rejection retains the server reason and allows retry without reconciling it away',async()=>{
    for(const status of [409,423]) {
        const f=fixture();await f.ui.open();
        const message=status===409?'폴더를 동기화하고 있습니다.':'DB가 잠겨 있습니다.';
        f.respond(async(_url,options)=>{
            if(options.method==='POST')throw Object.assign(new Error(message),{status});
            return response(idle());
        });
        await f.ui.start();
        assert.equal(f.get('integrity-status').textContent,message);
        assert.equal(f.get('integrity-start').disabled,false);assert.equal(f.ui.uncertain,false);
        assert.equal(f.calls.length,2,'an acknowledged rejection does not need a status recovery request');
        assert.equal(f.timers.size,0);
        f.respond(async()=>response(running()));await f.ui.start();
        assert.equal(f.ui.job.running,true);
    }
});
