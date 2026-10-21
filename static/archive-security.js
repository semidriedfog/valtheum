(() => {
    window.ArchiveSecurity = class {
        constructor({fetch, captureScope, isFresh, onChanged}) {
            Object.assign(this, {fetch, captureScope, isFresh, onChanged});
            this.monitor=new window.ArchiveJobMonitor(fetch);
            const get = id => document.getElementById(id);
            this.dialog=get('archive-security-dialog'); this.list=get('archive-security-list');
            this.status=get('archive-security-status'); this.search=get('archive-security-search');
            this.run=get('archive-security-encrypt'); this.cancel=get('archive-security-cancel');
            this.riskAck=get('media-encryption-risk-ack');
            this.riskAck.addEventListener('change',()=>this.updateActions());
            this.remove=get('archive-security-delete'); this.restore=get('archive-security-restore'); this.none=get('archive-security-none');
            this.remove.addEventListener('click',()=>this.batchAction('plaintext'));
            this.restore.addEventListener('click',()=>this.batchAction('export'));
            this.none.addEventListener('click',()=>{this.selected.clear();this.render();});
            this.close=get('archive-security-close'); this.all=get('archive-security-all');
            this.headerClose=get('archive-security-header-close'); this.countBadge=get('archive-security-count');
            if(this.headerClose) this.headerClose.addEventListener('click',()=>this.dismiss());
            this.progress=get('archive-security-progress');
            this.progressLabel=get('archive-security-progress-label');
            this.fileProgressSection=get('archive-security-file-progress-section');
            this.currentFileLabel=get('archive-security-current-file');
            this.fileProgress=get('archive-security-file-progress');
            this.fileProgressLabel=get('archive-security-file-progress-label');
            this.version=0; this.encryptionBatch=null; this.selected=new Set(); this.files=[]; this.pending=false;
            get('btn-archive-security').addEventListener('click',()=>this.open());
            this.search.addEventListener('input',()=>this.render());
            this.all.addEventListener('click',()=>{for(const f of this.filtered()) if(!f.trash)this.selected.add(f.id);this.render();});
            this.run.addEventListener('click',()=>this.batch());
            this.cancel.addEventListener('click',async()=>{
                if(!this.pending||this.cancel.disabled)return;
                const version=this.version,scope=this.scope,jobId=this.jobId;
                const attempt={};this.cancelAttempt=attempt;
                const fresh=()=>this.cancelAttempt===attempt&&this.version===version&&this.isFresh(scope)&&this.jobId===jobId&&this.pending;
                this.stopping=true;this.cancel.disabled=true;
                // If the start response is still pending, waitJob cancels its acknowledged ID.
                try{if(jobId)await this.fetch('/api/archive-security/cancel?job_id='+encodeURIComponent(jobId),{method:'POST',scope,reportError:false,discardBody:true});}
                catch(e){if(fresh()){this.status.textContent=e.message;this.cancel.disabled=false;}}
            });
            this.close.addEventListener('click',()=>this.dismiss());
            this.dialog.addEventListener('cancel',e=>{e.preventDefault();this.dismiss();});
        }
        isOpen(){return this.dialog.open;}
        dismiss(){if(this.pending)this.dialog.close();else this.reset();}
        reset(){this.monitor.stop();this.riskAck.checked=false;this.riskAck.disabled=false;this.jobId=null;this.version++;this.pending=false;this.stopping=true;this.selected.clear();this.files=[];this.list.replaceChildren();this.search.value='';this.status.textContent='';this.resetProgress();this.dialog.close();}
        filtered(){const q=this.search.value.toLowerCase().trim();return this.files.filter(f=>(f.display_name||f.name).toLowerCase().includes(q)||f.name.toLowerCase().includes(q));}
        async refresh(scope,version=this.version){const data=await(await this.fetch('/api/archive-security',{scope})).json();if(version!==this.version||!this.isFresh(scope))return null;this.files=data.files||[];this.render();return data;}
        async refreshViews(scope,version){
            if(version!==this.version||!this.isFresh(scope))return;
            // Refresh committed changes even after partial failure. A failed
            // security listing must not prevent the main list from updating.
            await Promise.allSettled([this.refresh(scope,version),this.onChanged(scope)]);
        }
        async open(){
            if(this.pending){this.dialog.showModal();return;}
            this.riskAck.checked=false;
            const version=++this.version,scope=this.captureScope();this.scope=scope;this.setPending(true);
            this.resetProgress();this.stopping=false;this.status.textContent=window.vt("불러오는 중…");this.dialog.showModal();
            try{
                const data=await this.refresh(scope,version);if(!data)return;
                let job=data.job;
                if(job?.running){this.setPending(true);job=await this.waitJob(scope,version,job.job_id);await this.refresh(scope,version);if(version!==this.version||!this.isFresh(scope))return;}
                this.status.textContent=job?.batch?`${this.batchSummary(job)}${window.VaultI18n.message(job.error)?'\n'+window.VaultI18n.message(job.error):''}`:window.vt("파일을 선택한 뒤 암호화 · 평문 원본 일괄 삭제 · 원본 일괄 복원을 실행하세요. 각 작업에 해당하는 선택 파일만 처리합니다.");
            }catch(e){if(version===this.version&&this.isFresh(scope))this.status.textContent=e.message;}
            finally{if(version===this.version)this.setPending(false);}
        }
        setPending(value){
            if(!value||!this.pending){this.cancelAttempt=null;this.cancel.disabled=false;}
            this.pending=value;this.riskAck.disabled=value;this.run.disabled=value;this.all.disabled=value;this.none.disabled=value;this.remove.disabled=value;this.restore.disabled=value;this.close.disabled=false;this.close.title=window.vt(value?"창을 닫아도 작업은 계속됩니다. 취소하려면 취소 버튼을 눌러 주세요.":"닫기");
            if(this.headerClose) this.headerClose.disabled=false;
            this.cancel.hidden=!value;this.render();
        }
        formatSize(bytes){
            if(!bytes||bytes<=0)return '';
            if(bytes<1048576)return `${(bytes/1024).toFixed(1)} KB`;
            if(bytes<1073741824)return `${(bytes/1048576).toFixed(1)} MB`;
            return `${(bytes/1073741824).toFixed(2)} GB`;
        }
        render(){
            this.list.replaceChildren();
            const files=this.filtered();
            if(files.length===0){
                const empty=document.createElement('div');empty.className='archive-empty-state';
                const icon=document.createElement('span');icon.className='archive-empty-icon';icon.textContent='📂';
                const msg=document.createElement('p');msg.textContent=this.files.length===0?window.vt("암호화할 영상·이미지·압축 파일이 없습니다."):window.vt("검색 결과와 일치하는 미디어가 없습니다.");
                empty.append(icon,msg);this.list.append(empty);this.updateActions();return;
            }
            for(const f of files.slice(0,200)){
                const row=document.createElement('div');
                const isSelected=this.selected.has(f.id);
                row.className=`archive-security-row${isSelected?' selected':''}${f.trash?' is-trash':''}`;

                const label=document.createElement('label');label.className='archive-file-label';
                const select=document.createElement('input');select.type='checkbox';select.className='archive-checkbox';select.checked=isSelected;select.disabled=this.pending||f.trash;
                select.addEventListener('change',()=>{
                    select.checked?this.selected.add(f.id):this.selected.delete(f.id);
                    row.classList.toggle('selected',select.checked);
                    this.updateActions();
                });
                label.append(select);

                const icon=document.createElement('span');icon.className='archive-type-icon';
                icon.textContent=f.state==='ready'?'🔒':f.file_type==='VIDEO'?'🎥':f.file_type==='IMAGE'?'🖼️':'📦';
                label.append(icon);

                const name=document.createElement('span');name.className='archive-file-name';name.textContent=f.display_name||f.name;name.title=f.display_name||f.name;label.append(name);
                if(f.size){
                    const size=document.createElement('span');size.className='archive-file-size';size.textContent=this.formatSize(f.size);label.append(size);
                }
                row.append(label);

                const badges=document.createElement('div');badges.className='archive-row-badges';
                if(f.state==='ready'){
                    const bReady=document.createElement('span');bReady.className='archive-badge badge-encrypted';bReady.textContent=window.vt("🔒 암호화됨");badges.append(bReady);
                    if(f.original){const bOrig=document.createElement('span');bOrig.className='archive-badge badge-original';bOrig.textContent=window.vt("원본 보존");badges.append(bOrig);}
                }else if(f.state==='pending'){
                    const bPending=document.createElement('span');bPending.className='archive-badge badge-pending';bPending.textContent=window.vt("⚠️ 중단됨");badges.append(bPending);
                }else{
                    const bPlain=document.createElement('span');bPlain.className='archive-badge badge-plaintext';bPlain.textContent=window.vt("평문");badges.append(bPlain);
                }
                if(f.trash){
                    const bTrash=document.createElement('span');bTrash.className='archive-badge badge-trash';bTrash.textContent=window.vt("🗑️ 휴지통");badges.append(bTrash);
                }
                row.append(badges);

                const actions=document.createElement('div');actions.className='archive-row-actions';
                if(f.state==='ready'){
                    const bRestore=document.createElement('button');bRestore.type='button';bRestore.className='btn secondary-btn archive-row-btn';bRestore.textContent=window.vt("원본 복원");bRestore.disabled=this.pending;
                    bRestore.addEventListener('click',()=>this.single(f,'export'));actions.append(bRestore);
                    if(f.original){
                        const bDel=document.createElement('button');bDel.type='button';bDel.className='btn secondary-btn archive-row-btn archive-btn-danger';bDel.textContent=window.vt("평문 원본 삭제");bDel.disabled=this.pending;
                        bDel.addEventListener('click',()=>this.single(f,'plaintext'));actions.append(bDel);
                    }
                }
                row.append(actions);
                this.list.append(row);
            }
            if(files.length>200){
                const note=document.createElement('div');note.className='archive-limit-notice';
                note.textContent=window.vt("ℹ️ 검색 결과 {0}개 중 상위 200개를 표시합니다. ('검색 결과 전체 선택'은 일치하는 {1}개 전체를 선택합니다)", files.length, files.length);
                this.list.append(note);
            }
            this.updateActions();
        }
        resetProgress(){
            this.encryptionBatch=null;this.progress.max=1;this.progress.value=0;
            this.progressLabel.textContent='';this.progressLabel.hidden=true;
            this.fileProgressSection.hidden=true;this.currentFileLabel.textContent='';
            this.fileProgressLabel.textContent='';this.fileProgress.max=1;this.fileProgress.value=0;
        }
        renderProgress(job={}){
            const batch=this.encryptionBatch||job.batch;
            const complete=!job.running&&job.completed&&!job.error;
            const phase=complete?window.vt("완료되었습니다."):(window.VaultI18n.message(job.phase)||window.vt("준비"));
            const total=Number(job.total),done=Math.max(0,Number(job.done)||0);
            const measurable=Number.isFinite(total)&&total>0;
            const fraction=complete?1:measurable?Math.min(1,done/total):null;
            const percent=fraction===null?null:Math.floor(fraction*100);
            const phaseText=percent===null?window.vt("{0} · 처리량 확인 중…",phase):
                window.vt("{0} · {1}%",phase,percent)+(measurable&&!complete?
                    ' · '+window.vt("{0} / {1}",this.formatSize(Math.min(done,total))||'0 B',this.formatSize(total)):'');
            this.progressLabel.hidden=false;
            this.fileProgressSection.hidden=!batch;
            const bar=batch?this.fileProgress:this.progress;
            bar.max=1;
            if(fraction===null)bar.removeAttribute('value');else bar.value=fraction;
            if(!batch){this.progressLabel.textContent=phaseText;return;}
            const count=this.encryptionBatch?batch.completed:batch.completed+batch.skipped+batch.failed;
            const processed=Math.max(0,Math.min(batch.total,count));
            this.progress.max=Math.max(1,batch.total);this.progress.value=processed;
            const overall=Math.floor(processed/Math.max(1,batch.total)*100);
            this.progressLabel.textContent=this.encryptionBatch?
                window.vt("전체 완료 {0}/{1}개 · {2}% (파일 수 기준)",processed,batch.total,overall):
                window.vt("전체 처리 {0}/{1}개 · {2}% (파일 수 기준)",processed,batch.total,overall);
            this.currentFileLabel.textContent=window.vt("현재 파일 {0}/{1}: {2}",
                this.encryptionBatch?batch.index:Math.min(batch.total,processed+(job.running?1:0)),batch.total,batch.current||'');
            this.fileProgressLabel.textContent=phaseText;
        }
        startJob(url,options,scope,version){
            return this.monitor.start(url,options,{scope,fresh:()=>version===this.version&&this.isFresh(scope),
                onUncertain:()=>{this.status.textContent=window.vt('작업 상태를 받지 못해 다시 확인하고 있습니다. 결과가 확인될 때까지 새 작업을 시작하지 않습니다. 취소하거나 창을 닫을 수 있습니다.');}});
        }
        async waitJob(scope,version=this.version,jobId){
            const fresh=()=>version===this.version&&this.isFresh(scope);
            if(!fresh())throw new Error(window.vt("폴더가 변경되었습니다."));
            if(!jobId)throw new Error(window.vt("작업 번호를 확인하지 못했습니다. 목록을 새로 확인해 주세요."));
            this.jobId=jobId;
            if(this.stopping){
                try{await this.fetch('/api/archive-security/cancel?job_id='+encodeURIComponent(jobId),{method:'POST',scope,reportError:false,discardBody:true});}
                catch(error){if(fresh()){this.cancel.disabled=false;this.status.textContent=error.message;}}
            }
            const job=await this.monitor.wait({jobId,scope,fresh,
                onProgress:job=>{this.renderProgress(job);this.status.textContent=this.stopping?window.vt('취소 중…'):(window.VaultI18n.message(job.phase)||window.vt('처리 중…'));},
                onUncertain:()=>{this.status.textContent=window.vt('작업 상태를 받지 못해 다시 확인하고 있습니다. 결과가 확인될 때까지 새 작업을 시작하지 않습니다. 취소하거나 창을 닫을 수 있습니다.');}
            });
            if(!job||!fresh())throw new Error(window.vt('폴더가 변경되었습니다.'));
            if(job.error)throw new Error(job.batch?this.batchSummary(job)+'\n'+window.VaultI18n.message(job.error):window.VaultI18n.message(job.error));
            return job;
        }
        confirmEncryption(count){
            if(!this.riskAck.checked){this.status.textContent=window.vt("암호화 위험 안내를 읽고 확인란을 선택해 주세요.");this.riskAck.focus();return false;}
            return confirm(window.vt("미디어 {0}개 암호화 전 확인\n\n• 복원하려면 해당 라이브러리의 vault.key와 비밀번호가 모두 필요합니다. 둘 중 하나라도 잃으면 암호화 파일을 복원할 수 없습니다.\n• 암호화 파일이 손상되면 키와 비밀번호가 있어도 복원이 실패할 수 있습니다.\n• 완료 후 DB 백업(.vtbackup: DB와 키)을 만들고, 암호화 미디어 파일도 별도로 다른 저장장치에 백업하세요. DB 백업에는 미디어가 포함되지 않습니다.\n• 평문 원본은 자동으로 삭제되지 않습니다. 암호화 결과와 백업을 확인한 뒤 원본 삭제 여부를 결정하세요.\n• 원본을 유지하므로 추가 저장 공간과 시간이 필요합니다. 작업 중 앱을 종료하거나 NAS·저장장치 연결을 끊지 마세요.\n\n위 내용을 확인했으며 선택한 미디어를 암호화하시겠습니까?",count));}
        async batch(){
            if(this.pending||!this.selected.size)return;
            const files=this.eligible('encrypt');if(!files.length||!this.confirmEncryption(files.length))return;
            const scope=this.captureScope(),version=this.version;this.scope=scope;this.jobId=null;this.stopping=false;this.resetProgress();this.setPending(true);
            const batch={total:files.length,completed:0,index:1,current:files[0].display_name||files[0].name};
            this.encryptionBatch=batch;this.renderProgress({running:true,phase:'준비'});
            try{
                for(const [index,file] of files.entries()){
                    if(this.stopping||!this.isFresh(scope))break;
                    batch.index=index+1;batch.current=file.display_name||file.name;
                    this.renderProgress({running:true,phase:'준비'});
                    this.status.textContent=window.vt("{0}/{1} 준비 중…",index+1,files.length);
                    this.jobId=null;
                    const started=await this.startJob('/api/archive-security/'+file.id+'/encrypt',{method:'POST'},scope,version);
                    const job=await this.waitJob(scope,version,started.job_id);
                    if(version!==this.version||!this.isFresh(scope))break;
                    if(!job.completed)throw new Error(window.vt("암호화 완료를 확인하지 못했습니다."));
                    this.selected.delete(file.id);batch.completed++;this.renderProgress(job);
                }
                if(version===this.version&&this.isFresh(scope)){
                    await this.refreshViews(scope,version);if(version!==this.version||!this.isFresh(scope))return;
                    if(version!==this.version||!this.isFresh(scope))return;
                    this.status.textContent=batch.completed===batch.total?
                        window.vt("{0}개 암호화 완료. DB 백업을 실행해 vault.key도 보관하세요. 원본 파일은 아직 남아 있습니다.",batch.completed):
                        window.vt("일괄 암호화를 취소했습니다. 완료 {0}/{1}개 · 남은 {2}개",batch.completed,batch.total,batch.total-batch.completed);
                }
            }catch(e){
                if(version===this.version&&this.isFresh(scope)){
                    this.status.textContent=window.vt("일괄 암호화 중단 · 완료 {0}/{1}개",batch.completed,batch.total)+'\n'+e.message;
                    await this.refreshViews(scope,version);
                }
            }finally{if(version===this.version)this.setPending(false);}
        }
        eligible(action){return this.files.filter(f=>this.selected.has(f.id)&&!f.trash&&(action==='encrypt'?f.state!=='ready':f.state==='ready'&&(action!=='plaintext'||f.original)));}
        updateActions(){
            const encrypt=this.eligible('encrypt'),remove=this.eligible('plaintext'),restore=this.eligible('export');
            this.run.textContent=window.vt("선택 {0}개 암호화", encrypt.length);
            this.run.title=window.vt("원본 보존에 필요한 추가 공간: 약 {0} MiB 이상", Math.ceil(encrypt.reduce((n,f)=>n+f.size,0)/1048576));
            this.remove.textContent=window.vt("평문 원본 일괄 삭제 ({0}개)", remove.length);
            this.restore.textContent=window.vt("원본 일괄 복원 ({0}개)", restore.length);
            this.run.disabled=this.pending||!encrypt.length||!this.riskAck.checked;this.remove.disabled=this.pending||!remove.length;this.restore.disabled=this.pending||!restore.length;
            if(this.countBadge){
                const total=this.selected.size;
                this.countBadge.textContent=window.vt("{0}개 선택", total);
                this.countBadge.classList.toggle('has-selection',total>0);
            }
        }
        batchSummary(job){const b=job.batch;if(!b)return window.vt("일괄 작업이 끝났습니다.");return window.vt("전체 {0}개 · 완료 {1}개 · 건너뜀 {2}개 · 실패 {3}개{4}", b.total, b.completed, b.skipped, b.failed, b.failures?.length?'\n'+b.failures.map(value=>window.VaultI18n.message(value)).join('\n'):'');}
        async batchAction(action){
            if(this.pending)return;
            const files=this.eligible(action);if(!files.length)return;
            if(action==='plaintext'&&!confirm(window.vt("선택한 {0}개 평문 원본을 일괄 삭제합니다. 파일마다 암호화 사본과 원본을 다시 검증합니다.\n\n{1}{2}\n\nDB·키와 암호화 파일 백업을 먼저 확인해 주세요. 이미 삭제를 완료한 원본은 작업 취소로 되돌아오지 않습니다. 계속할까요?", files.length, files.slice(0,10).map(f=>f.name).join('\n'), files.length>10?'\n…':'')))return;
            const scope=this.captureScope(),version=this.version;this.scope=scope;this.jobId=null;this.stopping=false;this.resetProgress();this.setPending(true);
            try{
                const started=await this.startJob('/api/archive-security/batch',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action,ids:files.map(f=>f.id)})},scope,version);
                if(started.status==='cancelled')return;
                const job=await this.waitJob(scope,version,started.job_id);
                if(version===this.version&&this.isFresh(scope)){await this.refreshViews(scope,version);if(version!==this.version||!this.isFresh(scope))return;this.status.textContent=window.vt("{0} 결과: {1}", action==='plaintext'?window.vt("평문 원본 일괄 삭제"):window.vt("원본 일괄 복원"), this.batchSummary(job));}
            }catch(e){if(version===this.version&&this.isFresh(scope)){await this.refreshViews(scope,version);if(version===this.version&&this.isFresh(scope))this.status.textContent=e.message;}}
            finally{if(version===this.version)this.setPending(false);}
        }
        async single(file,action){if(this.pending)return;if(action==='encrypt'&&!this.confirmEncryption(1))return;if(action==='plaintext'&&!confirm(window.vt("암호화 사본과 원본을 다시 검증한 뒤 평문 원본을 삭제합니다.\n{0}\n\n먼저 DB·키와 암호화 파일을 다른 저장장치에 백업하는 것을 권장합니다. 계속할까요?", file.name)))return;
            const scope=this.captureScope(),version=this.version;this.scope=scope;this.jobId=null;this.stopping=false;this.resetProgress();this.setPending(true);
            try{const data=await this.startJob(`/api/archive-security/${file.id}/${action}`,{method:action==='plaintext'?'DELETE':'POST'},scope,version);if(action==='plaintext')await this.waitJob(scope,version,data.job_id);else if(data.status==='started')await this.waitJob(scope,version,data.job_id);if(version===this.version&&this.isFresh(scope)){await this.refresh(scope,version);if(version!==this.version||!this.isFresh(scope))return;this.status.textContent=data.status==='cancelled'?window.vt("취소되었습니다."):window.vt("완료되었습니다.");}}
            catch(e){if(version===this.version&&this.isFresh(scope))this.status.textContent=e.message;}finally{if(version===this.version)this.setPending(false);}
        }
    };
})();
