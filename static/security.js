(() => {
    window.DatabaseSecurity = class {
        constructor({fetch, captureScope, isFresh, onChanged, onResync, onLocking}) {
            this.captureScope=captureScope; this.isFresh=isFresh;
            this.onLocking=onLocking; this.fetch = fetch; this.onChanged = onChanged; this.onResync = onResync;
            this.state = {encrypted:false, locked:false, plaintext_original:false};
            this.pending = false;
            const get = id => document.getElementById(id);
            this.dialog = get('db-security-dialog'); this.form = get('db-security-form');
            this.password = get('db-password'); this.confirmation = get('db-password-confirm');
            this.currentPassword = get('db-current-password'); this.currentRow = get('db-current-row');
            this.changeButton = get('db-change-password'); this.changing = false;
            this.changeButton.addEventListener('click', () => this.open(true));
            this.confirmRow = get('db-confirm-row'); this.title = get('db-security-title');
            this.note = get('db-security-note'); this.message = get('db-security-message');
            this.submit = get('db-security-submit'); this.close = get('db-security-close');
            this.risk = get('db-encryption-risk'); this.riskAck = get('db-encryption-risk-ack');
            this.riskAck.addEventListener('change', () => this.updateEncryptionAction());
            this.button = get('btn-db-security'); this.lockButton = get('btn-db-lock');
            this.cleanup = get('db-delete-plaintext'); this.status = get('db-security-status');
            this.button.addEventListener('click', () => this.open());
            get('btn-db-unlock').addEventListener('click', () => this.open());
            this.close.addEventListener('click', () => this.dismiss());
            this.dialog.addEventListener('cancel', e => { e.preventDefault(); this.dismiss(); });
            this.lockButton.addEventListener('click', () => this.perform('/api/security/lock'));
            this.cleanup.addEventListener('click', () => {
                if (confirm(window.vt("암호화 DB를 다시 검증한 뒤 이 폴더의 기존 평문 DB를 삭제합니다. 다른 위치에 만든 백업은 남습니다. 원본 미디어는 삭제하지 않습니다. 계속하시겠습니까?"))) {
                    this.perform('/api/security/plaintext', undefined, 'DELETE');
                }
            });
            this.form.addEventListener('submit', e => {
                e.preventDefault();
                if (this.pending || (this.state.encrypted && !this.state.locked && !this.changing)) return;
                if ((!this.state.encrypted || this.changing) && (Array.from(this.password.value).length < 10 || this.password.value !== this.confirmation.value)) {
                    this.message.textContent = window.vt("10자 이상 비밀번호를 입력하고 확인란에 똑같이 입력해 주세요."); return;
                }
                const data = {password:this.password.value, confirmation:this.confirmation.value};
                if (this.changing) {
                    if (!this.currentPassword.value) { this.message.textContent = window.vt("현재 비밀번호를 입력해 주세요."); return; }
                    data.current_password=this.currentPassword.value;
                }
                this.perform(this.changing ? '/api/security/password' : this.state.encrypted ? '/api/security/unlock' : '/api/security/encrypt', data);
            });
        }
        get locked() { return this.state.locked; }
        updateEncryptionAction() {
            this.submit.disabled = this.pending || (!this.state.encrypted && !this.riskAck.checked);
        }
        isOpen() { return this.dialog.open; }
        clear() { this.riskAck.checked = false; this.password.value = ''; this.confirmation.value = ''; this.currentPassword.value = ''; }
        dismiss() { if (this.pending) this.hiddenWhilePending = true; this.clear(); this.dialog.close(); }
        update(state) {
            if (!state) return;
            const wasLocked = this.state.locked;
            this.state = state;
            document.body.classList.toggle('db-locked', state.locked);
            this.button.textContent = state.encrypted ? window.vt("🔐 DB 보안") : window.vt("🔐 DB 암호화");
            this.lockButton.hidden = !state.encrypted || state.locked;
            this.status.textContent = state.locked ? window.vt("DB 잠김") : state.encrypted ?
                (state.plaintext_original ? window.vt("암호화됨 · 평문 원본 남음") : window.vt("DB 암호화됨")) : window.vt("DB 암호화 안 됨");
            for (const id of ['btn-backup-db','btn-archive-security','btn-start-arena','btn-open-trash','btn-series','btn-rescan-folder',
                'btn-reset-all-watch','btn-reset-all-scores','btn-reset-all-ratings']) {
                document.getElementById(id).disabled = state.locked;
            }
            if (state.locked && !wasLocked) this.open();
        }
        open(changePassword = false) {
            this.hiddenWhilePending = false;
            if (this.pending) { if (!this.dialog.open) this.dialog.showModal(); return; }
            this.clear(); this.message.textContent = '';
            const {encrypted, locked, plaintext_original} = this.state;
            this.changing = changePassword === true && encrypted && !locked;
            this.risk.hidden = encrypted;
            this.riskAck.checked = false;
            this.updateEncryptionAction();
            document.getElementById('db-password-label').textContent=this.changing?window.vt("새 비밀번호"):window.vt("비밀번호");
            document.getElementById('db-confirm-label').textContent=this.changing?window.vt("새 비밀번호 확인"):window.vt("비밀번호 확인");
            this.title.textContent = locked ? window.vt("DB 잠금 해제") : encrypted ? window.vt("DB 보안") : window.vt("기존 DB 암호화");
            this.note.textContent = locked ? window.vt("이 폴더의 DB 비밀번호를 입력해 주세요. 비밀번호는 저장하지 않습니다.") : encrypted ?
                (plaintext_original ? window.vt("암호화 DB를 사용 중입니다. 변환 전 평문 원본이 남아 있으므로 아래에서 삭제할 수 있습니다. 이전에 만든 별도 백업은 직접 관리해 주세요.") : window.vt("태그·시리즈·썸네일·시청 이력이 암호화되어 있습니다. 미디어 암호화는 별도의 압축 파일 암호화 화면에서 관리합니다.")) :
                window.vt("현재 폴더의 태그·시리즈·점수·썸네일·북마크를 암호화 DB로 복사하고 검증합니다. 원본 DB는 전환 후 직접 삭제할 때까지 보존됩니다. 미디어 파일은 암호화되지 않습니다. 비밀번호를 잊으면 암호화 DB를 복구할 수 없습니다.");
            this.form.hidden = encrypted && !locked;
            this.confirmRow.hidden = encrypted;
            this.confirmation.required = !encrypted;
            this.password.autocomplete = encrypted ? 'current-password' : 'new-password';
            this.submit.textContent = locked ? window.vt("잠금 해제") : window.vt("암호화 및 검증");
            this.cleanup.hidden = !encrypted || locked || !plaintext_original;
            this.changeButton.hidden = !encrypted || locked || this.changing;
            this.currentRow.hidden = !this.changing; this.currentPassword.required = this.changing;
            if (this.changing) {
                this.title.textContent=window.vt("폴더 비밀번호 변경");
                this.note.textContent=window.vt("현재 비밀번호와 새 비밀번호를 입력하세요. 아래 비밀번호·확인란에는 새 비밀번호를 입력합니다. DB와 vault.key에 함께 적용하며 미디어 파일은 다시 암호화하지 않습니다. 변경 후 DB·키 백업을 새로 만들어 주세요. 기존 백업은 이전 비밀번호로 열립니다.");
                this.form.hidden=false;this.confirmRow.hidden=false;this.confirmation.required=true;
                this.password.autocomplete='new-password';this.submit.textContent=window.vt("비밀번호 변경 및 검증");this.cleanup.hidden=true;
            }
            if (!this.dialog.open) this.dialog.showModal();
            if (this.changing) this.currentPassword.focus(); else if (!this.form.hidden) this.password.focus();
        }
        async perform(path, data, method = 'POST') {
            if (this.pending) return;
            if (path === '/api/security/encrypt' && !this.riskAck.checked) {
                this.message.textContent = window.vt("암호화 위험 안내를 읽고 확인란을 선택해 주세요.");
                this.riskAck.focus(); return;
            }
            if (path === "/api/security/encrypt" && !confirm(window.vt("DB 암호화 전 확인\n\n• 비밀번호를 잊으면 암호화된 DB를 열 수 없습니다. 비밀번호는 별도로 안전하게 보관하세요.\n• DB 암호화는 미디어 파일을 암호화하지 않습니다. 기존 평문 DB와 백업도 자동으로 삭제되지 않습니다.\n• 작업 전에 DB를 백업하고, 완료 후 새 DB 백업도 보관하세요. vault.key가 생성된 경우 DB 백업에 함께 포함됩니다.\n• 추가 저장 공간과 시간이 필요합니다. 작업 중 앱을 종료하거나 NAS·저장장치 연결을 끊지 마세요.\n\n위 내용을 확인했으며 DB 암호화를 시작하시겠습니까?"))) return;
            if(path==='/api/security/lock')this.onLocking?.();
            const scope = this.captureScope();
            this.clear();
            this.pending = true;
            this.hiddenWhilePending = false;
            this.message.textContent = window.vt("처리 중… 잠시 기다려 주세요.");
            for (const element of [this.submit,this.cleanup,this.lockButton,this.password,this.confirmation,this.currentPassword,this.changeButton,this.riskAck]) element.disabled = true;
            try {
                const options = {method, scope};
                if (data) { options.headers = {'Content-Type':'application/json'}; options.body = JSON.stringify(data); data.password = ''; data.confirmation = ''; data.current_password = ''; }
                let response;
                try { response = await this.fetch(path, options); }
                finally { options.body = undefined; }
                const payload = await response.json();
                if (!this.isFresh(scope)) return;
                this.clear(); this.dialog.close();
                const refreshed = await this.onChanged(payload);
                if (refreshed === null || !this.isFresh(refreshed || scope) || this.hiddenWhilePending) return;
                this.pending = false;
                if (payload.password_changed) { this.open(); this.message.textContent=window.VaultI18n.message(payload.message); }
                else if (payload.security?.encrypted && !payload.security.locked && payload.security.plaintext_original) this.open();
            } catch (err) {
                if (this.isFresh(scope) && err.name !== 'AbortError') {
                    // The server may have committed before the response was lost.
                    let refreshed = scope;
                    try { if(path!=='/api/security/lock')refreshed = await this.onResync?.(scope); } catch (_) { /* Keep the original error visible only in this session. */ }
                    if (refreshed === null || !this.isFresh(refreshed || scope)) return;
                    this.message.textContent = err.message;
                    this.status.textContent = err.message;
                    // Lock recovery stays inside the hidden entry dialog.
                }
            } finally {
                this.pending = false;
                for (const element of [this.submit,this.close,this.cleanup,this.lockButton,this.password,this.confirmation,this.currentPassword,this.changeButton,this.riskAck]) element.disabled = false;
                this.updateEncryptionAction();
                if (this.dialog.open && !this.form.hidden) this.password.focus();
            }
        }
    };
})();
