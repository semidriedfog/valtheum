const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

function fixture(language='ko') {
    const elements = new Map(), messages = [];
    class Element {
        constructor(){this.value='';this.children=[];this.dataset={};this.listeners={};}
        addEventListener(name,callback){this.listeners[name]=callback;}
        append(child){child.parent=this;this.children.push(child);}
        replaceChildren(){this.children=[];}
        removeAttribute(name){delete this[name];}
        remove(){this.parent.children=this.parent.children.filter(c=>c!==this);}
        get firstElementChild(){return this.children[0];}
        scrollIntoView(){}
    }
    const get=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
    const window={__valtheumLanguage:language,ipc:{postMessage:body=>messages.push(JSON.parse(body))}};
    const context=vm.createContext({window,document:{getElementById:get,createElement:()=>new Element()}});
    const html=fs.readFileSync(path.join(__dirname,'../restore-ui/index.html'),'utf8');
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../static/i18n-catalog.js'),'utf8'),context);
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../static/i18n.js'),'utf8'),context);
    vm.runInContext(html.match(/<script>([\s\S]*?)<\/script>/)[1],context);
    return {get,messages,receive:window.restoreUI.receive,submit:()=>get('restore-form').listeners.submit({preventDefault(){}})};
}
test('restorer requires all inputs, sends password once and clears it before work',()=>{
    const f=fixture();assert.equal(f.get('start').disabled,true);
    f.receive({type:'selection',key:'backup.vtbackup',folder:'output',files:['one.vta']});
    assert.equal(f.get('start').disabled,true);
    f.get('password').value='test-only password';f.get('password').listeners.input();
    assert.equal(f.get('start').disabled,false);f.submit();f.submit();
    assert.equal(f.messages.filter(m=>m.action==='start').length,1);
    assert.equal(f.get('password').value,'');assert.equal(f.get('pick-key').disabled,true);
    f.receive({type:'started'});f.get('cancel').listeners.click();
    assert.equal(f.messages.at(-1).action,'cancel');assert.equal(f.get('cancel').disabled,true);
    f.receive({type:'finished',value:{success:1,failed:0,cancelled:true}});
    assert.match(f.get('status').textContent,/취소됨.*성공 1개/);
    assert.equal(f.get('pick-key').disabled,false);assert.equal(f.get('start').disabled,true);
});
test('restorer reports failures safely, bounds long lists and permits password retry',()=>{
    const f=fixture(), name='<img src=x onerror=alert(1)>.vta';
    f.receive({type:'selection',key:'vault.key',folder:'output',files:Array(250).fill(name)});
    assert.equal(f.get('file-list').children.length,200);
    assert.equal(f.get('file-list').children[0].textContent,name);
    f.receive({type:'error',message:'키가 맞지 않습니다.'});
    assert.equal(f.get('status').dataset.tone,'error');assert.equal(f.get('password').disabled,false);
    for(let i=0;i<305;i++)f.receive({type:'file',value:{source:name,error:'기존 파일은 보존됩니다.'}});
    assert.equal(f.get('results').children.length,300);
    assert.equal(f.get('results').children[0].textContent,'실패 · '+name);
    f.receive({type:'finished',value:{success:0,failed:305,cancelled:false}});
    assert.match(f.get('status').textContent,/실패 305개/);
    assert.equal(f.get('status').dataset.tone,'error');
});

for(const language of ['en','ja']) test(`restore errors and results use ${language} and preserve filenames`,()=>{
    const f=fixture(language),name='완료_日本語.vtv';
    f.receive({type:'selection',key:'vault.key',folder:'출력',files:[name]});
    assert.equal(f.get('file-list').children[0].textContent,name);
    f.receive({type:'error',message:'비밀번호/키가 맞지 않거나 파일이 손상되었습니다.'});
    assert.match(f.get('status').textContent,language==='en'?/Wrong password/:/パスワード/);
    f.receive({type:'file',value:{source:name,error:'대상 파일이 이미 있습니다.'}});
    assert.equal(f.get('results').children[0].textContent,(language==='en'?'Failed':'失敗')+' · '+name);
});


