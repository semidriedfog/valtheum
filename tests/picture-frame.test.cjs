const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

function fixture() {
    const elements = new Map(), timers = new Map(), events = {}, messages = [], saves = [], errors = [];
    let sequence = 0;
    class Element {
        constructor() {
            this.listeners = {}; this.children = []; this.hidden = false; this.value = '';
            const classes = new Set();
            this.classList = {add: x => classes.add(x), remove: x => classes.delete(x)};
        }
        addEventListener(name, fn) { this.listeners[name] = fn; }
        replaceChildren(...children) { this.children = children; }
        focus() {}
    }
    const get = id => { if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id); };
    const document = {getElementById:get, addEventListener:(name,fn)=>events[name]=fn,
        exitFullscreen:async()=>{document.fullscreenElement=null;events.fullscreenchange();}};
    const window = {__valtheumLanguage:'ko',ipc:{postMessage:message=>messages.push(message)}};
    const context = vm.createContext({window, document, Image:Element,
        setTimeout:(fn,ms)=>{timers.set(++sequence,{fn,ms});return sequence;}, clearTimeout:id=>timers.delete(id)});
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../static/i18n-catalog.js'),'utf8'),context);
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../static/i18n.js'),'utf8'),context);
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../static/picture-frame.js'),'utf8'),context);
    const frame = new window.PictureFrame({canStart:()=>true,
        getImages:()=>[{name:'1.webp',src:'/one'},{name:'2.webp',src:'/two'}],
        saveSeconds:async seconds=>saves.push(seconds), onError:message=>errors.push(message)});
    const fire = id => {const task=timers.get(id);assert.ok(task);timers.delete(id);task.fn();};
    const loaded = () => get('frame-canvas').children[0].onload();
    return {frame,get,document,window,events,timers,messages,saves,errors,fire,loaded};
}
test('frame waits for image load, uses the chosen duration, loops, pauses and clears timers on exit',async()=>{
    const f=fixture(),p=f.frame; p.setSeconds(7); await p.start();
    assert.deepEqual(f.messages,['frame:enter']); assert.equal(p.index,0);
    assert.equal(f.timers.has(p.timer),false,'no slideshow clock until image is loaded');
    f.loaded(); assert.equal(f.timers.get(p.timer).ms,7000);
    f.fire(p.timer); assert.equal(p.index,1); f.loaded();
    f.fire(p.timer); assert.equal(p.index,0); f.loaded();
    p.togglePause(); assert.equal(f.timers.has(p.timer),false);
    p.move(1); f.loaded(); assert.equal(f.timers.has(p.timer),false);
    p.togglePause(); assert.equal(f.timers.get(p.timer).ms,7000);
    const lateLoad=f.get('frame-canvas').children[0].onload;
    p.stop(); lateLoad(); assert.equal(f.timers.size,0);
    assert.deepEqual(f.messages,['frame:enter','frame:exit']);
    assert.equal(f.get('privacy-layout-content').inert,false);
});
test('duration changes restart the clock, synchronize controls and save; invalid input is rejected',async()=>{
    const f=fixture(),p=f.frame;await p.start();f.loaded();
    const input=f.get('frame-seconds');input.value='23';await p.changeSeconds(input);
    assert.equal(f.timers.get(p.timer).ms,23000);assert.deepEqual(f.saves,[23]);
    assert.equal(f.get('privacy-frame-seconds').value,'23');
    for(const value of ['0','3601','NaN','1.5','']) {input.value=value;await p.changeSeconds(input);}
    assert.deepEqual(f.saves,[23]);assert.equal(p.seconds,23);assert.equal(f.errors.length,5);
    p.stop();
});
test('failed or stalled images advance and late image callbacks cannot change the current timer',async()=>{
    const f=fixture(),p=f.frame;await p.start();
    const stale=f.get('frame-canvas').children[0].onload;
    f.fire(p.loadTimer);assert.equal(f.get('frame-status').hidden,false);
    f.fire(p.timer);const nextVersion=p.version;stale();assert.equal(p.ready,false);
    f.get('frame-canvas').children[0].onerror();assert.equal(p.version,nextVersion);assert.ok(f.timers.has(p.timer));
    p.stop();
});
test('Escape leaves native fullscreen, browser fullscreen failure cleans up and an empty gallery cannot start',async()=>{
    const f=fixture(),p=f.frame;await p.start();
    let prevented=false;
    p.handleKey({key:'Escape',preventDefault(){prevented=true;},stopImmediatePropagation(){}});
    assert.equal(prevented,true);assert.equal(p.active,false);assert.equal(f.timers.size,0);
    delete f.window.ipc;f.get('picture-frame').requestFullscreen=async()=>{throw Error('denied');};
    await p.start();assert.equal(p.active,false);assert.equal(f.errors.length,1);assert.equal(f.timers.size,0);
    p.getImages=()=>[];await p.start();assert.equal(p.active,false);
});
test('exiting while browser fullscreen is pending restores the browser as well',async()=>{
    const f=fixture(),p=f.frame;delete f.window.ipc;let finish;
    f.get('picture-frame').requestFullscreen=()=>new Promise(resolve=>finish=()=>{f.document.fullscreenElement=p.root;resolve();});
    const pending=p.start();p.stop();finish();await pending;
    assert.equal(f.document.fullscreenElement,null);assert.equal(p.active,false);assert.equal(f.timers.size,0);
});
