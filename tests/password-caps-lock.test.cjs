const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');
const read=p=>fs.readFileSync(path.join(__dirname,'..',p),'utf8');
function fixture(language='ko') {
    class Element {
        constructor(id){this.id=id;this.attrs={};this.dataset={};this.listeners={};}
        setAttribute(k,v){this.attrs[k]=v;}
        getAttribute(k){return this.attrs[k]||null;}
        addEventListener(k,f){this.listeners[k]=f;}
        insertAdjacentElement(position,node){assert.equal(position,'afterend');this.status=node;}
    }
    const inputs=['privacy-password','pdf-password','db-restore-password','db-current-password','db-password','db-password-confirm'].map(id=>new Element(id));
    inputs[0].setAttribute('aria-describedby','existing-help');
    // The indicator must never inspect or retain password contents.
    inputs.forEach(input=>Object.defineProperty(input,'value',{get(){throw Error('password read');}}));
    const document={readyState:'complete',activeElement:null,listeners:{},querySelectorAll:()=>inputs,createElement:()=>new Element(),addEventListener(k,f){this.listeners[k]=f;}};
    const window={__valtheumLanguage:language,listeners:{},addEventListener(k,f){this.listeners[k]=f;}};
    const context=vm.createContext({window,document});
    vm.runInContext(read('static/i18n-catalog.js'),context);vm.runInContext(read('static/i18n.js'),context);
    vm.runInContext(read('static/password-caps-lock.js'),context);
    return {inputs,document,window,focus(input){document.activeElement=input;input.listeners.focus();},event(name,on){document.listeners[name]({getModifierState(key){assert.equal(key,'CapsLock');return on;}});}};
}
test('every password field reports on/off from keyboard and mouse without reading the password',()=>{
    const f=fixture();
    for(const input of f.inputs){
        assert.equal(input.status.hidden,true);f.focus(input);assert.equal(input.status.dataset.state,'unknown');
        f.event('keydown',true);assert.equal(input.status.dataset.state,'on');assert.equal(input.status.textContent,'Caps Lock 켜짐');
        f.event('keyup',false);assert.equal(input.status.dataset.state,'off');assert.equal(input.status.textContent,'Caps Lock 꺼짐');
        f.event('click',true);assert.equal(input.status.dataset.state,'on');
        input.listeners.blur();assert.equal(input.status.hidden,true);assert.equal(input.status.textContent,'');
    }
    assert.equal(f.inputs[0].getAttribute('aria-describedby'),'existing-help privacy-password-caps-lock');
    assert.equal(f.inputs[0].status.getAttribute('role'),'status');
});
test('returning from another window invalidates stale Caps Lock state',()=>{
    const f=fixture(),input=f.inputs[0];f.focus(input);f.event('keydown',true);
    f.window.listeners.blur();assert.equal(input.status.dataset.state,'unknown');
    f.window.listeners.focus();assert.equal(input.status.dataset.state,'unknown');
    f.document.listeners.keydown({});assert.equal(input.status.dataset.state,'unknown');
    f.event('keyup',false);assert.equal(input.status.dataset.state,'off');
    f.document.listeners.visibilitychange();assert.equal(input.status.dataset.state,'unknown');
    input.listeners.blur();f.document.activeElement={};f.event('keydown',true);assert.equal(input.status.hidden,true);
});
for(const [language,on,off] of [['en','Caps Lock is on','Caps Lock is off'],['ja','Caps Lock オン','Caps Lock オフ']]){
    test('Caps Lock status is translated in '+language,()=>{
        const f=fixture(language),input=f.inputs[0];f.focus(input);f.event('keydown',true);assert.equal(input.status.textContent,on);
        f.event('keyup',false);assert.equal(input.status.textContent,off);
    });
}
test('both application and standalone restorer embed the shared password indicator',()=>{
    assert.match(read('static/index.html'),/src="password-caps-lock\.js/);
    assert.match(read('static/index.html'),/href="password-caps-lock\.css/);
    const rust=read('src/restore_ui.rs'),html=read('restore-ui/index.html');
    for(const part of ['style','script'])assert.ok(html.includes('<!--caps-lock-'+part+'-->'));
    assert.ok(rust.includes('include_str!("../static/password-caps-lock.js")'));
    assert.ok(rust.includes('include_str!("../static/password-caps-lock.css")'));
});
