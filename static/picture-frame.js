(() => {
    window.PictureFrame = class {
        constructor({canStart, getImages, saveSeconds, onError}) {
            Object.assign(this, {canStart, getImages, saveSeconds, onError});
            const get = id => document.getElementById(id);
            this.root = get('picture-frame'); this.canvas = get('frame-canvas');
            this.status = get('frame-status'); this.position = get('frame-position');
            this.startButton = get('privacy-frame'); this.pauseButton = get('frame-pause');
            this.inputs = [get('privacy-frame-seconds'), get('frame-seconds')];
            this.active = false; this.paused = false; this.seconds = 10; this.version = 0;
            this.startButton.addEventListener('click', () => {
                if (Number(this.inputs[0].value) !== this.seconds) void this.changeSeconds(this.inputs[0]);
                return this.start();
            });
            get('frame-exit').addEventListener('click', () => this.stop());
            get('frame-prev').addEventListener('click', () => this.move(-1));
            get('frame-next').addEventListener('click', () => this.move(1));
            this.pauseButton.addEventListener('click', () => this.togglePause());
            for (const input of this.inputs) input.addEventListener('change', () => this.changeSeconds(input));
            this.root.addEventListener('pointermove', () => this.wakeControls());
            this.root.addEventListener('pointerdown', () => this.wakeControls());
            document.addEventListener('keydown', event => this.handleKey(event));
            document.addEventListener('fullscreenchange', () => {
                if (this.active && !this.native && document.fullscreenElement !== this.root) this.stop();
            });
            window.addEventListener?.('pagehide', () => this.stop());
            this.setSeconds(10);
        }
        setSeconds(value) {
            this.seconds = Number.isInteger(value) && value >= 1 && value <= 3600 ? value : 10;
            for (const input of this.inputs) input.value = String(this.seconds);
            this.schedule();
        }
        async changeSeconds(input) {
            const value = Number(input.value), previous = this.seconds;
            if (!Number.isInteger(value) || value < 1 || value > 3600) {
                this.setSeconds(previous);
                this.onError(window.vt("표시 시간은 1~3,600초의 정수로 입력해 주세요."));
                return;
            }
            this.setSeconds(value);
            for (const field of this.inputs) field.disabled = true;
            try { await this.saveSeconds(value); }
            catch (error) {
                if (error.name !== 'AbortError') this.onError(window.vt("표시 시간을 저장하지 못했습니다. 이번 재생에는 적용됩니다."));
            }
            finally { for (const field of this.inputs) field.disabled = false; }
        }
        async start() {
            if (this.active || !this.canStart()) return;
            this.images = this.getImages();
            if (!this.images.length) return;
            this.active = true; this.paused = false; this.index = 0; this.native = !!window.ipc?.postMessage;
            this.root.hidden = false; this.pauseButton.textContent = window.vt("일시정지");
            document.getElementById('privacy-layout-content').inert = true;
            this.root.focus(); this.wakeControls(); this.show();
            try {
                if (this.native) window.ipc.postMessage('frame:enter');
                else await this.root.requestFullscreen();
                // An Esc/lock during an asynchronous fullscreen request must still exit it.
                if (!this.active && document.fullscreenElement === this.root) await document.exitFullscreen();
            } catch (_) {
                this.stop(); this.onError(window.vt("전체 화면을 시작하지 못했습니다. 액자 모드를 다시 눌러 주세요."));
            }
        }
        stop() {
            if (!this.active) return;
            this.active = false; ++this.version;
            clearTimeout(this.timer); clearTimeout(this.loadTimer); clearTimeout(this.controlsTimer);
            this.canvas.replaceChildren(); this.images = []; this.ready = false;
            this.root.hidden = true;
            document.getElementById('privacy-layout-content').inert = false;
            if (this.native) window.ipc.postMessage('frame:exit');
            else if (document.fullscreenElement === this.root) document.exitFullscreen().catch(() => {});
            this.startButton.focus();
        }
        show() {
            clearTimeout(this.timer); clearTimeout(this.loadTimer);
            this.ready = false;
            const version = ++this.version, sample = this.images[this.index];
            const image = new Image(); image.alt = sample.name; image.draggable = false;
            this.status.textContent = window.vt("불러오는 중…"); this.status.hidden = false;
            this.position.textContent = `${this.index + 1} / ${this.images.length}`;
            const finish = failed => {
                if (!this.active || version !== this.version || this.ready) return;
                clearTimeout(this.loadTimer); this.ready = true;
                image.hidden = failed; this.status.hidden = !failed;
                this.status.textContent = failed ? window.vt("이 사진을 표시할 수 없습니다. 다음 사진으로 넘어갑니다.") : '';
                this.schedule();
            };
            image.onload = () => finish(false); image.onerror = () => finish(true);
            this.canvas.replaceChildren(image);
            this.loadTimer = setTimeout(() => finish(true), 15000);
            image.src = sample.src;
        }
        schedule() {
            clearTimeout(this.timer);
            if (this.active && this.ready && !this.paused && this.images.length > 1) {
                this.timer = setTimeout(() => this.move(1), this.seconds * 1000);
            }
        }
        move(delta) {
            if (!this.active || this.images.length < 2) return;
            this.index = (this.index + delta + this.images.length) % this.images.length;
            this.show();
        }
        togglePause() {
            if (!this.active) return;
            this.paused = !this.paused;
            this.pauseButton.textContent = this.paused ? window.vt("재생") : window.vt("일시정지");
            this.schedule(); this.wakeControls();
        }
        wakeControls() {
            if (!this.active) return;
            this.root.classList.remove('frame-idle'); clearTimeout(this.controlsTimer);
            this.controlsTimer = setTimeout(() => this.root.classList.add('frame-idle'), 2500);
        }
        handleKey(event) {
            if (!this.active) return;
            if (event.key === 'Escape') {
                event.preventDefault(); event.stopImmediatePropagation(); this.stop(); return;
            }
            this.wakeControls();
            if (event.altKey || event.ctrlKey || event.metaKey || event.target?.closest?.('input,select,textarea')) return;
            if (event.key === 'ArrowLeft' || event.key === 'ArrowRight' || event.code === 'Space') {
                event.preventDefault(); event.stopImmediatePropagation();
                if (event.code === 'Space') this.togglePause();
                else this.move(event.key === 'ArrowLeft' ? -1 : 1);
            }
        }
    };
})();
