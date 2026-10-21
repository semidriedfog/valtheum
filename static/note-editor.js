(() => {
    window.NoteEditor = class {
        constructor({fetch,captureScope,isFresh,onChanged,onSetupEncryption,confirmEncryption,canEdit=()=>true}) {
            Object.assign(this,{fetch,captureScope,isFresh,onChanged,onSetupEncryption,confirmEncryption,canEdit});
            this.exportMonitor=new window.ArchiveJobMonitor(fetch);
            this.el={};for(const id of ['dialog','file','view','edit','export','close','cancel','editor','title','body','save','viewport','space','canvas','zoom','whitespace','status','progress','setup'])this.el[id]=document.getElementById('notes-'+id);
            this.version=0;
            this.el.close.addEventListener('click',()=>this.close());
            this.el.dialog.addEventListener('cancel',e=>{e.preventDefault();this.close();});
            this.el.view.addEventListener('click',()=>this.read(false));this.el.edit.addEventListener('click',()=>this.read(true));
            this.el.export.addEventListener('click',()=>this.export());
            this.el.editor.addEventListener('submit',e=>{e.preventDefault();this.submit();});
            for(const field of ['title','body'])this.el[field].addEventListener('input',()=>this.notifyExitRisk());
            this.el.zoom.addEventListener('change',()=>this.layout());this.el.whitespace.addEventListener('change',()=>this.paint());
            this.el.viewport.addEventListener('scroll',()=>this.paint());
            this.el.setup.addEventListener('click',()=>{if(!this.pending&&this.discard()){this.reset();this.onSetupEncryption();}});
            this.el.cancel.addEventListener('click',async()=>{
                if(!this.jobStarted||!this.jobId||this.el.cancel.disabled)return;
                const version=this.version,scope=this.scope,jobId=this.jobId;
                const fresh=()=>this.fresh(version)&&this.jobId===jobId&&this.pending&&this.jobStarted;
                this.el.cancel.disabled=true;
                try{await this.fetch('/api/archive-security/cancel?job_id='+encodeURIComponent(jobId),{method:'POST',scope,discardBody:true});}
                catch(error){if(fresh()&&this.isOpen())this.el.status.textContent=error.message;}
                finally{if(fresh())this.el.cancel.disabled=false;}
            });
            if(typeof ResizeObserver!=='undefined')new ResizeObserver(()=>{if(this.isOpen()&&this.text!==undefined)this.layout();}).observe(this.el.viewport);
            this.reset();
        }
        isOpen(){return this.el.dialog.open;}
        allowEditing(){
            if(this.canEdit())return true;
            alert(window.vt('DB 작업이 진행 중입니다. 작업이 끝난 뒤 메모를 작성하거나 수정해 주세요.'));
            return false;
        }
        hasDraft(){return !this.el.editor.hidden&&!this.pending&&!this.saveAttempt&&!this.detached;}
        hasChanges(){return !this.el.editor.hidden&&(this.el.title.value!==this.originalTitle||this.el.body.value!==this.originalBody);}
        exitRisk(){return this.hasChanges()||!!this.saveAttempt||(this.pending&&!this.reading);}
        notifyExitRisk(){
            const risk=this.exitRisk();
            // Only a boolean leaves the WebView; note contents stay in memory here.
            window.ipc?.postMessage(risk?'notes:dirty':'notes:clean');
        }
        canAutoSelect(){return !this.isOpen()&&!this.pending&&!this.saveAttempt&&!this.detached;}
        fresh(version){return version===this.version&&this.isFresh(this.scope);}
        clearContent(){this.originalTitle='';this.originalBody='';this.el.title.value='';this.el.body.value='';this.el.editor.hidden=true;this.text=undefined;this.lines=[];this.el.canvas.width=1;this.el.canvas.height=1;this.el.space.style.height='0px';this.el.viewport.hidden=true;}
        reset(){this.exportMonitor.stop();this.el.cancel.disabled=false;this.jobId=null;this.requestController?.abort();this.requestController=null;this.reading=false;this.detached=false;if(this.saveAttempt){this.saveAttempt.payload.title='';this.saveAttempt.payload.body='';}this.saveAttempt=null;this.version++;this.file=null;this.revision=null;this.pending=false;this.jobStarted=false;this.clearContent();this.el.file.textContent='';this.el.status.textContent='';this.el.progress.hidden=true;this.el.setup.hidden=true;this.controls();this.el.dialog.close();}
        discard(){if(this.saveAttempt)return confirm(window.vt('저장 결과가 아직 확인되지 않았습니다. 저장되었을 수 있으니 목록을 확인해 주세요. 창을 닫을까요?'));return !this.hasChanges()||confirm(window.vt('작성 중인 메모를 저장하지 않고 닫을까요?'));}
        leave(){
            // Background navigation must not dismiss an ordinary unsaved edit,
            // including an existing note whose text has been emptied.
            if(this.hasDraft())return false;
            if((this.pending&&!this.reading)||this.saveAttempt||this.detached){this.detached=true;this.el.dialog.close();}else this.reset();
            return true;
        }
        canOpen(file=null){
            if(this.hasDraft()&&(this.file?.id??null)!==(file?.id??null))return false;
            const retained=(this.pending&&!this.reading)||this.saveAttempt||this.detached;
            if(retained&&(this.file?.id??null)!==(file?.id??null)){
                alert(window.vt('현재 메모의 작업 또는 저장 결과 확인을 먼저 마쳐 주세요. 해당 메모를 다시 열어 확인할 수 있습니다.'));
                return false;
            }
            return true;
        }
        resume(){if(this.reading)return false;if(!this.hasDraft()&&!this.pending&&!this.saveAttempt&&!this.detached)return false;this.detached=false;this.el.dialog.showModal();return true;}
        close(){if(this.pending&&!this.reading){this.leave();return;}if(!this.discard())return;this.reset();}
        controls(){
            this.notifyExitRisk();
            if(!this.pending)this.el.progress.hidden=true;
            for(const name of ['title','body','setup'])this.el[name].disabled=this.pending||!!this.saveAttempt;
            for(const name of ['save','zoom','whitespace'])this.el[name].disabled=this.pending;
            this.el.close.disabled=false;
            this.el.close.title=window.vt(this.pending&&!this.reading?'창을 닫아도 작업은 계속됩니다. 취소하려면 취소 버튼을 눌러 주세요.':'닫기');
            for(const name of ['view','edit','export'])this.el[name].disabled=this.pending||!!this.saveAttempt||!this.file;
            this.el.save.textContent=window.vt(this.saveAttempt&&!this.pending?'저장 결과 확인':'암호화해 저장');
            this.el.cancel.hidden=!this.jobStarted;
            if(!this.jobStarted)this.el.cancel.disabled=false;
        }
        async requestJson(url, options={}) {
            const controller=new AbortController();this.requestController=controller;
            let timer,abort;
            const interrupted=new Promise((_,reject)=>{
                abort=()=>reject(new Error(window.vt('요청을 취소했습니다.')));
                controller.signal.addEventListener('abort',abort,{once:true});
                timer=setTimeout(()=>{
                    reject(new Error(window.vt('응답 대기 시간(15초)을 초과했습니다. 다시 시도하거나 창을 닫아 주세요.')));
                    controller.abort();
                },15000);
            });
            try {
                // Include body consumption in the deadline, not just response headers.
                return await Promise.race([(async()=>await(await this.fetch(url,{...options,scope:this.scope,signal:controller.signal})).json())(),interrupted]);
            } finally {
                clearTimeout(timer);controller.signal.removeEventListener('abort',abort);
                if(this.requestController===controller)this.requestController=null;
            }
        }
        openNew(){if(!this.allowEditing()||!this.canOpen())return;if(this.resume())return;this.reset();this.scope=this.captureScope();this.el.file.textContent=window.vt('새 메모');this.el.editor.hidden=false;this.el.dialog.showModal();this.controls();this.el.title.focus();}
        async open(file){if(!this.canOpen(file))return;if(this.resume())return;if(file?.file_type!=='NOTE'||file.missing)return;this.reset();this.file=file;this.scope=this.captureScope();this.el.file.textContent=file.display_name||window.vt('제목 없는 메모');this.el.dialog.showModal();await this.read(false);}
        async read(edit){
            if(this.pending||!this.file||(edit&&!this.allowEditing())||!this.discard())return;
            const version=++this.version;this.clearContent();this.pending=true;this.reading=true;this.controls();
            this.el.status.textContent=window.vt('처리 중…');
            try{
                const note=await this.requestJson('/api/notes/'+this.file.id);
                if(!this.fresh(version))return;
                this.revision=note.revision;this.el.file.textContent=note.title||window.vt('제목 없는 메모');
                if(edit){this.el.title.value=note.title;this.el.body.value=note.body;this.originalTitle=this.el.title.value;this.originalBody=this.el.body.value;this.el.editor.hidden=false;this.el.body.focus();}
                else{this.text=note.body;this.layout();}
                note.body='';this.el.status.textContent='';
            }catch(error){if(this.fresh(version))this.el.status.textContent=error.message;}
            finally{if(this.fresh(version)){this.pending=false;this.reading=false;this.controls();}}
        }
        layout(){if(this.text===undefined)return;this.el.viewport.hidden=false;this.font=Math.max(14,Math.min(32,Number(this.el.zoom.value)||18));this.lineHeight=Math.ceil(this.font*1.65);this.width=Math.max(180,(this.el.space.clientWidth||this.el.viewport.clientWidth)-24);const ctx=this.el.canvas.getContext('2d');ctx.font=`${this.font}px Consolas, 'Cascadia Mono', monospace`;this.lines=[];
            // Keep the stored string unchanged. Grapheme wrapping affects only the canvas.
            const segmenter=typeof Intl.Segmenter==='function'?new Intl.Segmenter(undefined,{granularity:'grapheme'}):null;
            const rawLines=this.text.split(/\r\n|\n|\r/);for(const [lineIndex,raw]of rawLines.entries()){let line='',width=0;const parts=segmenter?[...segmenter.segment(raw)].map(x=>x.segment):Array.from(raw);for(const part of parts){const shown=part==='\t'?'    ':part;const w=ctx.measureText(shown).width;if(line&&width+w>this.width){this.lines.push({text:line,end:false});line='';width=0;}line+=part;width+=w;}this.lines.push({text:line,end:lineIndex<rawLines.length-1});}
            this.el.space.style.height=Math.max(this.lineHeight,this.lines.length*this.lineHeight+24)+'px';this.el.viewport.scrollTop=0;this.paint();}
        paint(){if(this.text===undefined||!this.lines?.length)return;const top=this.el.viewport.scrollTop||0;const first=Math.max(0,Math.floor(top/this.lineHeight)-1);const count=Math.min(this.lines.length-first,Math.ceil((this.el.viewport.clientHeight||480)/this.lineHeight)+3);const height=count*this.lineHeight+24;const density=Math.min(window.devicePixelRatio||1,2);const canvas=this.el.canvas;canvas.width=Math.ceil((this.width+24)*density);canvas.height=Math.ceil(height*density);canvas.style.width=(this.width+24)+'px';canvas.style.height=height+'px';canvas.style.top=(first*this.lineHeight)+'px';const ctx=canvas.getContext('2d');ctx.scale(density,density);ctx.fillStyle='#141416';ctx.fillRect(0,0,this.width+24,height);ctx.fillStyle='#f4f4f5';ctx.font=`${this.font}px Consolas, 'Cascadia Mono', monospace`;ctx.textBaseline='top';for(let i=0;i<count;i++){const row=this.lines[first+i];let line=row.text;line=line.replace(/\t/g,this.el.whitespace.checked?'→   ':'    ');if(this.el.whitespace.checked)line=line.replace(/ /g,'·')+(row.end?'↵':'');ctx.fillText(line,12,i*this.lineHeight+8);}}
        async wait(version, requestId=null, jobId=this.jobId){
            if(!requestId&&!jobId)throw new Error(window.vt("작업 번호를 확인하지 못했습니다. 목록을 새로 확인해 주세요."));
            for(;;){
                const data=await this.requestJson(requestId?'/api/note-saves/'+requestId:'/api/archive-security/progress?job_id='+encodeURIComponent(jobId));
                if(!this.fresh(version))return null;
                if(data.status==='unknown')return null;
                const job=data.job||{};
                if(!requestId&&job.job_id!==jobId)throw new Error(window.vt("작업 번호가 일치하지 않습니다."));
                if(job.job_id)this.jobId=job.job_id;
                this.el.progress.hidden=false;this.el.progress.max=job.total||1;this.el.progress.value=job.done||0;
                this.el.status.textContent=window.VaultI18n.message(job.phase)||window.vt('처리 중…');
                if(!job.running){
                    if(job.error){const error=new Error(window.VaultI18n.message(job.error));error.jobFailed=true;throw error;}
                    if(!job.completed)throw Error(window.vt('완료 여부를 확인하지 못했습니다. 메모 목록을 다시 열어 확인해 주세요.'));
                    return job;
                }
                this.jobStarted=true;this.controls();
                await new Promise(resolve=>setTimeout(resolve,400));
                if(!this.fresh(version))return null;
            }
        }
        async submit(){if(this.pending||!this.allowEditing())return;
            if(this.saveAttempt)return this.saveOrReconcile();
            if(/[\uD800-\uDFFF]/u.test(this.el.title.value+this.el.body.value)){this.el.status.textContent=window.vt('올바르지 않은 유니코드 문자가 포함되어 있습니다. 해당 문자를 수정해 주세요.');return;}
            if(new TextEncoder().encode(this.el.title.value).length>512||new TextEncoder().encode(this.el.body.value).length>262144){this.el.status.textContent=window.vt('메모 제목은 512바이트, 본문은 256 KiB 이하여야 합니다.');return;}
            if(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F\u200B-\u200F\u202A-\u202E\u2060-\u206F]/u.test(this.el.body.value)&&!confirm(window.vt('보이지 않는 제어 문자가 포함되어 있습니다. 원문은 보존되지만 화면 표시가 달라질 수 있습니다. 저장할까요?')))return;
            if(!this.file&&!this.confirmEncryption())return;
            const bytes=crypto.getRandomValues(new Uint8Array(16));
            const request_id=Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('');
            this.saveAttempt={path:'/api/notes'+(this.file?'/'+this.file.id:''),sent:false,
                payload:{request_id,revision:this.revision,title:this.el.title.value,body:this.el.body.value}};
            await this.saveOrReconcile();
        }
        async saveOrReconcile(){
            const version=this.version,attempt=this.saveAttempt;
            let mayHaveSaved=attempt.sent;
            this.jobId=null;this.el.cancel.disabled=false;this.pending=true;this.controls();this.el.setup.hidden=true;
            try{
                let job=attempt.sent?await this.wait(version,attempt.payload.request_id):null;
                if(!this.fresh(version))return;
                if(!job){
                    attempt.sent=true;
                    const started=await this.requestJson(attempt.path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(attempt.payload)});
                    mayHaveSaved=true;
                    if(!this.fresh(version))return;
                    this.jobId=started.job_id;this.jobStarted=true;this.controls();
                    job=await this.wait(version,attempt.payload.request_id);
                }
                if(!this.fresh(version))return;
                const id=job?.result_id;
                if(!id)throw Error(window.vt('메모 저장 결과를 확인하지 못했습니다.'));
                // Keep the acknowledged identity even if the media list cannot refresh.
                this.file={...this.file,id,file_type:'NOTE',display_name:attempt.payload.title};
                this.revision=null;
                this.saveAttempt=null;attempt.payload.body='';attempt.payload.title='';
                this.clearContent();this.detached=false;this.pending=false;this.jobStarted=false;this.controls();
                await this.onChanged(this.scope,id,{reveal:this.isOpen()});
            }catch(error){
                if(this.fresh(version)){
                    // A lost response is not evidence that the write failed. Retain the
                    // exact payload/ID until its own result is known; never create another note.
                    if(error.jobFailed||(!mayHaveSaved&&[400,401,403,404,413,422,423].includes(error.status))){
                        this.saveAttempt=null;attempt.payload.body='';attempt.payload.title='';
                    }
                    this.el.status.textContent=error.message+(this.saveAttempt?'\n'+window.vt('저장 여부를 확인하지 못했습니다. 저장 결과 확인을 눌러 확인해 주세요.'): '');
                    this.el.setup.hidden=!!this.file||!!this.saveAttempt;
                }
            }finally{if(this.fresh(version)){this.pending=false;this.jobStarted=false;this.controls();}}
        }
        async export(){
            if(this.pending||this.saveAttempt||!this.file||!confirm(window.vt('메모를 평문 JSON 파일로 내보냅니다. 저장 위치와 접근 권한을 확인해 주세요. 계속할까요?')))return;
            const version=this.version,scope=this.scope,id=this.file.id;
            const fresh=()=>this.fresh(version);
            const onUncertain=()=>{this.el.status.textContent=window.vt('작업 상태를 받지 못해 다시 확인하고 있습니다. 결과가 확인될 때까지 새 작업을 시작하지 않습니다. 취소하거나 창을 닫을 수 있습니다.');};
            this.jobId=null;this.jobStarted=false;this.pending=true;this.controls();
            this.el.status.textContent=window.vt('처리 중…');
            try{
                const data=await this.exportMonitor.start('/api/notes/'+id+'/export',{method:'POST',scope},{scope,fresh,onUncertain});
                if(!fresh()||!data||data.status==='cancelled')return;
                this.jobId=data.job_id;this.jobStarted=true;this.controls();
                const job=await this.exportMonitor.wait({jobId:data.job_id,scope,fresh,onUncertain,
                    onProgress:job=>{
                        this.el.progress.hidden=false;this.el.progress.max=job.total||1;this.el.progress.value=job.done||0;
                        this.el.status.textContent=window.VaultI18n.message(job.phase)||window.vt('처리 중…');
                    }
                });
                if(!fresh()||!job)return;
                if(job.error)throw new Error(window.VaultI18n.message(job.error));
                if(!job.completed)throw new Error(window.vt('완료 여부를 확인하지 못했습니다. 메모 목록을 다시 열어 확인해 주세요.'));
                this.el.status.textContent=window.vt('메모 내보내기 작업이 끝났습니다.');
            }catch(error){if(fresh())this.el.status.textContent=error.message;}
            finally{if(fresh()){this.pending=false;this.jobStarted=false;this.controls();}}
        }
    };
})();
