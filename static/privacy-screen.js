(() => {
    window.PrivacyScreen = class {
        constructor({fetch, mediaUrl, captureScope, isFresh, onRevealed, onResync, onPrepareEntry, onCancelEntry}) {
            Object.assign(this,{fetch,mediaUrl,captureScope,isFresh,onRevealed,onResync,onPrepareEntry,onCancelEntry});
            const get=id=>document.getElementById(id);
            this.dialog=get('privacy-dialog');this.password=get('privacy-password');this.message=get('privacy-message');
            this.submit=get('privacy-submit');this.close=get('privacy-close');
            this.folder=get('privacy-folder');this.folders=[];this.loading=false;this.entryVersion=0;this.picking=false;
            this.retry=get('privacy-retry');this.retry.addEventListener('click',()=>this.openEntry());
            this.entryReady=false;this.lockUnconfirmed=false;this.closeUnconfirmed=false;
            this.pickFolder=get('privacy-pick-folder');this.passwordRow=get('privacy-password-row');
            this.pickFolder.addEventListener('click',()=>this.chooseLibrary());
            this.folder.addEventListener('change',()=>{this.password.value='';this.message.textContent=this.entryHint();this.updateEntryControls();if(!this.password.disabled)this.password.focus();});
            this.covered=false;this.pending=false;this.clicks=0;this.started=0;
            this.search=get('privacy-search');this.sort=get('privacy-sort');this.list=get('privacy-list');
            this.image=get('privacy-image');this.filename=get('privacy-filename');this.position=get('privacy-position');
            this.count=get('privacy-count');this.empty=get('privacy-empty');this.listEmpty=get('privacy-list-empty');
            this.samples=[];this.galleryVersion=0;this.galleryBusy=false;this.galleryStatus=window.vt("사진 폴더를 선택해 주세요.");
            this.choose=get('privacy-choose');this.refresh=get('privacy-refresh');this.clear=get('privacy-clear');
            this.galleryMessage=get('privacy-gallery-message');
            this.frame=new window.PictureFrame({
                canStart:()=>this.covered&&!this.galleryBusy&&!this.isOpen(),
                getImages:()=>[...this.samples].sort((a,b)=>(this.sort.value==='name-desc'?-1:1)*a.name.localeCompare(b.name,window.VaultI18n.locale,{numeric:true})),
                saveSeconds:async seconds=>{await this.fetch('/api/gallery/frame-settings',{method:'POST',discardBody:true,headers:{'Content-Type':'application/json'},body:JSON.stringify({seconds})});},
                onError:message=>{this.galleryMessage.textContent=message;this.frame.status.textContent=message;this.frame.status.hidden=false;}
            });
            this.choose.addEventListener('click',()=>this.loadGallery('select'));
            this.refresh.addEventListener('click',()=>this.loadGallery());
            this.clear.addEventListener('click',()=>this.loadGallery('clear'));
            this.image.addEventListener('error',()=>{if(this.covered){this.image.hidden=true;this.empty.hidden=false;this.empty.textContent=window.vt("이미지를 표시할 수 없습니다.");}});
            this.prev=get('privacy-prev');this.next=get('privacy-next');this.selected=null;this.visible=[];
            this.search.addEventListener('input',()=>this.renderGallery());
            this.sort.addEventListener('change',()=>this.renderGallery());
            this.prev.addEventListener('click',()=>this.move(-1));
            this.next.addEventListener('click',()=>this.move(1));
            document.addEventListener('keydown',event=>this.handleKey(event));
            get('privacy-library-title').addEventListener('click',()=>{
                if(!this.covered||this.pending||this.isOpen()||this.frame.active)return;
                const now=performance.now();
                if(now-this.started>2500){this.clicks=0;this.started=now;}
                if(this.clicks===0)this.started=now;
                if(++this.clicks>=5){this.clicks=0;return this.openEntry();}
            });
            this.close.addEventListener('click',()=>this.dismiss());
            get('privacy-header-close')?.addEventListener('click',()=>this.dismiss());
            this.dialog.addEventListener('cancel',e=>{e.preventDefault();this.dismiss();});
            get('privacy-form').addEventListener('submit',e=>{e.preventDefault();this.reveal();});
        }
        isOpen(){return this.dialog.open;}
        dismiss(){
            if(this.picking)return;
            const closing=this.pending?this.onCancelEntry?.():undefined;
            this.clearEntry();this.dialog.close();this.clicks=0;
            return closing;
        }
        clearEntry(){
            this.entryVersion++;this.entryReady=false;this.retry.hidden=true;this.folders=[];this.loading=false;this.folder.replaceChildren();this.folder.value='';
            this.password.value='';this.message.textContent='';this.updateEntryControls();
        }
        entryHint(){
            if(!this.folders.length)return window.vt("폴더 지정을 눌러 사용할 라이브러리를 선택해 주세요.");
            return this.folders.find(folder=>folder.path===this.folder.value)?.encrypted===false?
                window.vt("암호화되지 않은 폴더입니다. 열기를 누르면 라이브러리로 사용합니다."):'';
        }
        renderFolders(selected){
            this.folder.replaceChildren(...this.folders.map(folder=>{
                const option=document.createElement('option');option.value=folder.path;
                option.textContent=`${folder.name} — ${folder.path}`;return option;
            }));
            this.folder.value=this.folders.some(folder=>folder.path===selected)?selected:(this.folders[0]?.path??'');
        }
        updateEntryControls(){
            const selected=this.folders.find(folder=>folder.path===this.folder.value),ready=!!selected;
            const busy=this.loading||this.pending||this.picking,encrypted=selected?.encrypted!==false;
            this.folder.disabled=busy||!this.folders.length;this.pickFolder.disabled=busy||!this.entryReady;this.retry.disabled=busy;
            this.passwordRow.hidden=ready&&!encrypted;this.password.required=ready&&encrypted;
            this.password.disabled=busy||!ready||!encrypted;
            this.submit.disabled=busy||!ready;this.close.disabled=this.picking;
        }
        async chooseLibrary(){
            if(!this.entryReady||!this.covered||!this.isOpen()||this.pending||this.loading||this.picking)return;
            const scope=this.captureScope(),version=this.entryVersion;
            this.picking=true;this.password.value='';this.updateEntryControls();
            try{
                const data=await(await this.fetch('/api/security/select-folder',{method:'POST'})).json();
                if(!this.covered||!this.isOpen()||version!==this.entryVersion||!this.isFresh(scope))return;
                if(data.status==='selected'&&data.folder){
                    this.folders=this.folders.filter(folder=>folder.path!==data.folder.path);
                    this.folders.push(data.folder);this.renderFolders(data.folder.path);
                }
                this.message.textContent=this.entryHint();
            }catch(e){if(this.covered&&this.isOpen()&&version===this.entryVersion&&this.isFresh(scope)&&e.name!=='AbortError')this.message.textContent=window.vt("폴더를 선택하지 못했습니다. 다시 시도해 주세요.");}
            finally{this.picking=false;this.updateEntryControls();if(this.isOpen()&&!this.password.disabled)this.password.focus();}
        }
        async openEntry(){
            if(this.loading||this.pending||this.picking||!this.covered)return;
            this.cancelGallery();
            this.frame.stop();
            this.clearEntry();this.dialog.showModal();this.loading=true;this.updateEntryControls();
            this.message.textContent=window.vt("폴더 목록을 불러오는 중…");
            let scope=this.captureScope();const version=this.entryVersion;
            const entryFresh=()=>this.covered&&this.dialog.open&&version===this.entryVersion;
            const fresh=()=>entryFresh()&&this.isFresh(scope);
            try{
                for(let attempt=0;attempt<2;attempt++){
                    try{
                        if(this.lockUnconfirmed||this.closeUnconfirmed)await this.onPrepareEntry?.(entryFresh);
                        if(!entryFresh())return;
                        scope=this.captureScope();
                        const data=await(await this.fetch('/api/security/folders')).json();
                        if(!entryFresh())return;
                        if(!fresh())throw Object.assign(new Error('Session changed'),{name:'AbortError'});
                        this.entryReady=true;this.folders=Array.isArray(data.folders)?data.folders:[];
                        this.renderFolders(data.active_folder);
                        this.message.textContent=this.entryHint();
                        break;
                    }catch(e){
                        // Startup can invalidate the first folder request. Retry once
                        // against the new session, without reviving a dismissed dialog.
                        if(attempt===0&&entryFresh()&&!this.isFresh(scope))continue;
                        throw e;
                    }
                }
            }catch(e){if(entryFresh()){
                this.retry.hidden=false;
                this.message.textContent=(this.lockUnconfirmed||this.closeUnconfirmed)?window.vt("라이브러리 닫기 또는 잠금 완료를 확인하지 못했습니다. 잠시 후 다시 시도해 주세요. 화면은 계속 숨겨집니다."):window.vt("폴더 목록을 불러오지 못했습니다. 다시 시도해 주세요.");
            }}
            finally{if(entryFresh()){this.loading=false;this.updateEntryControls();if(this.folders.length&&!this.password.disabled)this.password.focus();}}
        }
        setCovered(value){
            const newlyCovered=!!value&&!this.covered;
            this.covered=!!value;this.clicks=0;this.password.value='';
            document.body.classList.toggle('privacy-covered',this.covered);
            if(newlyCovered){this.search.value='';this.sort.value='default';this.selected=null;this.renderGallery();}
            if(!value){
                this.frame.stop();
                this.cancelGallery();this.clearEntry();this.dialog.close();
                this.choose.disabled=this.refresh.disabled=this.clear.disabled=false;
                this.samples=[];this.visible=[];this.list.replaceChildren();this.image.removeAttribute('src');
            }
        }
        cancelGallery(){
            this.galleryVersion++;
            this.galleryCancel?.();this.galleryCancel=null;
            this.galleryBusy=false;this.galleryAction=null;
            this.choose.disabled=this.refresh.disabled=this.clear.disabled=false;
            this.frame.startButton.disabled=!this.samples.length;
        }
        async loadGallery(action='list'){
            if(!this.covered||(this.galleryBusy&&this.galleryAction==='select')||(this.isOpen()&&action!=='list'))return;
            this.cancelGallery();
            this.frame.stop();this.frame.startButton.disabled=true;
            const scope=this.captureScope(),version=++this.galleryVersion;
            this.galleryBusy=true;this.galleryAction=action;
            this.choose.disabled=this.refresh.disabled=this.clear.disabled=action==='select';
            const controller=new AbortController();let timer;
            const interrupted=new Promise((_,reject)=>{
                this.galleryCancel=()=>{controller.abort();reject(Object.assign(new Error('Gallery request cancelled'),{name:'AbortError'}));};
                // The native picker may stay open as long as the user needs it.
                // Its subsequent NAS scan has a separate server-side deadline.
                if(action!=='select')timer=setTimeout(()=>{
                    controller.abort();reject(new Error('Gallery request timed out'));
                },15000);
            });
            this.galleryMessage.textContent=action==='select'?window.vt("사진 폴더를 선택해 주세요."):window.vt("사진 목록을 불러오는 중…");
            try{
                const path=action==='select'?'/api/gallery/select':'/api/gallery';
                const options=action==='select'?{method:'POST'}:action==='clear'?{method:'DELETE'}:{};
                const data=await Promise.race([
                    (async()=>await(await this.fetch(path,{...options,signal:controller.signal})).json())(),
                    interrupted
                ]);
                if(!this.covered||version!==this.galleryVersion||!this.isFresh(scope))return;
                if(!data.cancelled){
                    this.applyGallery(data);
                    if(action!=='list'){this.search.value='';this.selected=null;this.renderGallery();}
                }
                this.galleryMessage.textContent=this.galleryStatus;
            }catch(e){
                if(this.covered&&version===this.galleryVersion&&this.isFresh(scope)&&e.name!=='AbortError'){
                    if(action==='list'){this.samples=[];this.renderGallery();}
                    this.galleryMessage.textContent=window.vt("사진 폴더를 읽지 못했습니다. 다시 선택해 주세요.");
                }
            }finally{
                clearTimeout(timer);
                if(version===this.galleryVersion){this.galleryCancel=null;this.galleryAction=null;this.galleryBusy=false;this.choose.disabled=this.refresh.disabled=this.clear.disabled=false;this.frame.startButton.disabled=!this.samples.length;}
            }
        }
        applyGallery(data){
            this.frame.stop();this.frame.setSeconds(data.frame_seconds??10);
            this.samples=(Array.isArray(data.images)?data.images:[]).map(sample=>({...sample,src:this.mediaUrl(sample.src),description:sample.name}));
            this.frame.startButton.disabled=!this.samples.length;
            this.galleryFolder=data.folder_name;
            this.galleryStatus=data.status==='unavailable'?window.vt("사진 폴더를 찾을 수 없습니다. 저장 장치 연결을 확인해 주세요."):
                data.status==='partial'?window.vt("일부 사진을 읽지 못했습니다. 새로고침해 주세요."):
                data.folder_name?window.vt("{0} · {1}개", data.folder_name, this.samples.length):window.vt("사진 폴더를 선택해 주세요.");
            this.galleryMessage.textContent=this.galleryStatus;this.renderGallery();
        }
        renderGallery(){
            if(!this.covered)return;
            const query=this.search.value.trim().toLocaleLowerCase(window.VaultI18n.locale);
            this.visible=this.samples.filter(sample=>sample.name.toLocaleLowerCase(window.VaultI18n.locale).includes(query));
            if(this.sort.value==='default'||this.sort.value==='name'||this.sort.value==='name-desc'){
                const direction=this.sort.value==='name-desc'?-1:1;
                this.visible.sort((a,b)=>direction*a.name.localeCompare(b.name,window.VaultI18n.locale,{numeric:true}));
            }
            if(!this.visible.some(sample=>sample.id===this.selected))this.selected=this.visible[0]?.id??null;
            this.count.textContent=window.vt("{0}개", this.visible.length);
            this.listEmpty.hidden=this.visible.length>0;
            const emptyText=this.search.value.trim()?window.vt("검색 결과가 없습니다."):this.samples.length?window.vt("선택한 이미지 없음"):
                !this.galleryFolder?this.galleryStatus:window.vt("이 폴더에 표시할 사진이 없습니다.");
            this.empty.textContent=emptyText;this.listEmpty.textContent=emptyText;
            this.list.replaceChildren(...this.visible.map(sample=>{
                const card=document.createElement('button');card.type='button';card.className='privacy-card';card.dataset.id=sample.id;
                const thumbnail=document.createElement('img');thumbnail.src=sample.src;thumbnail.alt='';thumbnail.loading='lazy';thumbnail.draggable=false;thumbnail.addEventListener('error',()=>{thumbnail.hidden=true;});
                const label=document.createElement('span');label.textContent=sample.name;
                card.append(thumbnail,label);
                card.addEventListener('click',()=>{if(this.covered&&!this.isOpen()){this.selected=sample.id;this.renderSelection();}});
                return card;
            }));
            this.renderSelection();
        }
        renderSelection(){
            const index=this.visible.findIndex(sample=>sample.id===this.selected),sample=this.visible[index];
            this.image.hidden=!sample;this.empty.hidden=!!sample;
            if(sample){this.image.src=sample.src;this.image.alt=sample.description;}
            else {this.image.removeAttribute('src');this.image.alt='';}
            this.filename.textContent=sample?.name??window.vt("선택한 이미지 없음");
            this.position.textContent=`${index+1} / ${this.visible.length}`;
            this.prev.disabled=index<=0;this.next.disabled=index<0||index>=this.visible.length-1;
            for(const card of this.list.children){
                const active=card.dataset.id===this.selected;
                card.classList.toggle('active',active);card.setAttribute('aria-pressed',String(active));
            }
        }
        move(delta){
            if(!this.covered||this.isOpen())return;
            const index=this.visible.findIndex(sample=>sample.id===this.selected),next=this.visible[index+delta];
            if(index<0||!next)return;
            this.selected=next.id;this.renderSelection();
            for(const card of this.list.children)if(card.dataset.id===this.selected)card.scrollIntoView({block:'nearest'});
        }
        handleKey(event){
            if(this.frame.active)return;
            if(!this.covered||this.isOpen()||event.altKey||event.ctrlKey||event.metaKey||event.shiftKey)return;
            if(event.target?.closest?.('input,textarea,select,[contenteditable="true"]'))return;
            if(event.key==='ArrowLeft'||event.key==='ArrowRight'){event.preventDefault();this.move(event.key==='ArrowLeft'?-1:1);}
        }
        async reveal(){
            const selected=this.folders.find(folder=>folder.path===this.folder.value);
            if(this.pending||this.picking||this.loading||!this.covered||!this.isOpen()||!selected||(selected.encrypted!==false&&!this.password.value))return;
            const scope=this.captureScope();
            const options={method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path:this.folder.value,password:this.password.value})};
            this.password.value='';this.pending=true;this.updateEntryControls();
            this.message.textContent=window.vt("확인 중…");
            try{
                let response;try{response=await this.fetch('/api/security/open-folder',options);}finally{options.body=undefined;}
                const data=await response.json();if(!this.isFresh(scope))return;
                const refreshed=await this.onRevealed(data);
                if(refreshed===null||!this.isFresh(refreshed||scope))return;
                this.message.textContent='';this.dialog.close();
            }catch(e){
                if(this.isFresh(scope)&&e.name!=='AbortError'){
                    // Resync without showing server errors, filenames or storage paths.
                    let refreshed=scope;
                    try{refreshed=await this.onResync?.(scope);}catch(_){}
                    if(refreshed===null||!this.isFresh(refreshed||scope))return;
                    if(this.covered)this.message.textContent=e.status===423?window.vt("다른 앱이나 PC에서 이 라이브러리를 사용 중이거나 저장소 잠금을 사용할 수 없습니다. 다른 앱에서 라이브러리를 닫고 다시 시도해 주세요."):window.vt("선택한 폴더와 비밀번호를 확인해 주세요.");
                }
            }finally{
                this.password.value='';this.pending=false;this.updateEntryControls();
                if(this.dialog.open)this.password.focus();
            }
        }
    };
})();
