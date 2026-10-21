(() => {
    window.BackupRestore = class {
        constructor({fetch, captureScope, isFresh, onChanged, onResync}) {
            Object.assign(this,{fetch,captureScope,isFresh,onChanged,onResync});
            const get=id=>document.getElementById(id);
            this.dialog=get('db-restore-dialog');this.select=get('db-restore-select');this.file=get('db-restore-file');
            this.password=get('db-restore-password');this.passwordRow=get('db-restore-password-row');
            this.form=get('db-restore-form');this.submit=get('db-restore-submit');this.close=get('db-restore-close');this.status=get('db-restore-status');
            this.pending=false;this.selection=null;
            get('btn-restore-db').addEventListener('click',()=>{if(!this.pending&&!this.hiddenWhilePending)this.dismiss();this.hiddenWhilePending=false;if(!this.dialog.open)this.dialog.showModal();});
            this.select.addEventListener('click',()=>this.choose());
            this.close.addEventListener('click',()=>this.dismiss());
            this.dialog.addEventListener('cancel',e=>{e.preventDefault();this.dismiss();});
            this.form.addEventListener('submit',e=>{e.preventDefault();this.restore();});
        }
        dismiss(){
            if(this.pending){this.hiddenWhilePending=true;this.password.value='';this.dialog.close();return;}
            const token=this.selection?.token;this.reset();
            if(token)this.fetch('/api/backup/restore/clear',{method:'POST',discardBody:true,headers:{'Content-Type':'application/json'},body:JSON.stringify({token})}).catch(()=>{});
        }
        isOpen(){return this.dialog.open;}
        reset(){this.password.value='';this.selection=null;this.file.textContent=window.vt("선택한 파일 없음");this.passwordRow.hidden=true;this.password.required=false;this.status.textContent='';this.submit.disabled=true;this.dialog.close();}
        setPending(value){if(value&&!this.pending)this.hiddenWhilePending=false;this.pending=value;this.select.disabled=value;this.close.disabled=false;this.password.disabled=value;this.submit.disabled=value||!this.selection;}
        async choose(){
            if(this.pending)return;
            const scope=this.captureScope();this.password.value='';this.setPending(true);this.status.textContent=window.vt("백업 파일을 선택해 주세요.");
            try{
                const data=await(await this.fetch('/api/backup/restore/select',{method:'POST'})).json();
                if(!this.isFresh(scope))return;
                if(data.status==='cancelled'){this.status.textContent=window.vt("파일 선택을 취소했습니다.");return;}
                this.selection=data;this.scope=scope;
                this.file.textContent=`${data.name}\n${data.encrypted?window.vt("암호화된 DB"):window.vt("평문 DB")} · ${data.key_included?window.vt("vault.key 포함"):window.vt("DB만 포함")}`;
                this.passwordRow.hidden=!data.encrypted;this.password.required=data.encrypted;
                this.status.textContent=data.encrypted?window.vt("백업 당시 비밀번호를 입력하세요. 이후 변경한 비밀번호와 다를 수 있습니다."):window.vt("저장 위치를 선택하면 새 폴더에 복원합니다.");
            }catch(e){if(this.isFresh(scope))this.status.textContent=e.message;}
            finally{this.setPending(false);if(this.selection?.encrypted&&this.dialog.open)this.password.focus();}
        }
        async restore(){
            if(this.pending||!this.selection)return;
            if(!this.isFresh(this.scope)){this.reset();return;}
            if(this.selection.encrypted&&!this.password.value){this.status.textContent=window.vt("백업 당시 비밀번호를 입력해 주세요.");return;}
            const scope=this.captureScope();
            const options={method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({token:this.selection.token,password:this.password.value})};
            this.password.value='';this.setPending(true);this.status.textContent=window.vt("상위 폴더를 선택해 주세요. 선택 후 DB·키를 검증하고 복원합니다. 큰 DB는 시간이 걸릴 수 있습니다.");
            try{
                let response;try{response=await this.fetch('/api/backup/restore',options);}finally{options.body=undefined;}
                const data=await response.json();if(!this.isFresh(scope))return;
                if(data.status==='cancelled'){this.status.textContent=window.vt("저장 위치 선택을 취소했습니다.");return;}
                this.selection=null;
                const refreshed=data.status==='success'?await this.onChanged(data):scope;
                if(refreshed===null||!this.isFresh(refreshed||scope))return;
                if(!this.hiddenWhilePending&&!this.dialog.open)this.dialog.showModal();
                this.passwordRow.hidden=true;this.password.required=false;
                this.status.textContent=window.vt("DB 백업 복원 완료\n{0}\n{1}\n미디어 파일은 별도로 원래 경로에 복사한 뒤 동기화해 주세요.", data.restore_path, window.VaultI18n.message(data.message)||window.vt("복원된 라이브러리를 열었습니다."));
            }catch(e){
                if(this.isFresh(scope)&&e.name!=='AbortError'){
                    let refreshed=scope;
                    try{refreshed=await this.onResync?.(scope);}catch(_){}
                    if(refreshed===null||!this.isFresh(refreshed||scope))return;
                    this.status.textContent=window.vt("복원 결과를 확인해 주세요: {0}", e.message);
                }
            }finally{this.password.value='';this.setPending(false);}
        }
    };
})();
