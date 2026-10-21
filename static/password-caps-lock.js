(() => {
    'use strict';
    function initialize() {
        const fields = new Map();
        for (const input of document.querySelectorAll('input[type="password"]')) {
            const status = document.createElement('span');
            status.id = `${input.id}-caps-lock`;
            status.className = 'password-caps-lock';
            status.hidden = true;
            status.setAttribute('role', 'status');
            status.setAttribute('aria-live', 'polite');
            status.setAttribute('data-no-i18n', '');
            input.insertAdjacentElement('afterend', status);
            const describedBy = input.getAttribute('aria-describedby');
            input.setAttribute('aria-describedby', [describedBy, status.id].filter(Boolean).join(' '));
            fields.set(input, status);
            input.addEventListener('focus', () => render(input, null));
            input.addEventListener('blur', () => { status.hidden = true; status.textContent = ''; });
        }
        function render(input, state) {
            const status = fields.get(input);
            if (!status) return;
            status.hidden = false;
            status.dataset.state = state === null ? 'unknown' : state ? 'on' : 'off';
            status.textContent = window.vt(state === null ? 'Caps Lock: 키를 누르면 상태를 확인합니다.'
                : state ? 'Caps Lock 켜짐' : 'Caps Lock 꺼짐');
        }
        function update(event) {
            if (!fields.has(document.activeElement) || typeof event.getModifierState !== 'function') return;
            render(document.activeElement, event.getModifierState('CapsLock'));
        }
        // A focus event has no modifier state. Never reuse a possibly stale state
        // after switching windows; read the keyboard/mouse event instead.
        for (const name of ['keydown', 'keyup', 'click']) document.addEventListener(name, update, true);
        const reset = () => render(document.activeElement, null);
        window.addEventListener('blur', reset);
        window.addEventListener('focus', reset);
        document.addEventListener('visibilitychange', reset);
    }
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initialize, { once: true });
    else initialize();
})();
