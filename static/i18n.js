(() => {
    'use strict';
    const supported = ['ko', 'en', 'ja'];
    let language = supported.includes(window.__valtheumLanguage) ? window.__valtheumLanguage : 'en';
    const catalog = window.VaultMessages || {};
    const records = [];
    const t = (source, ...args) => {
        const pattern = catalog[language]?.[source] ?? source;
        return pattern.replace(/\{(\d+)\}/g, (match, index) => index < args.length ? String(args[index]) : match);
    };
    // Only application-owned messages enter this function. Never translate media names or tags.
    const templates = Object.keys(catalog.en || {}).filter(k => /[가-힣]/.test(k) && /\{[^}]*\}/.test(k)).map(key => {
        const tokens = [...key.matchAll(/\{[^}]*\}/g)].map(m => m[0]);
        const parts = key.split(/\{[^}]*\}/g).map(s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
        return {key,tokens,regex:new RegExp('^'+parts.join('([\\s\\S]*?)')+'$')};
    });
    function message(source, depth = 0) {
        if (typeof source !== 'string' || language === 'ko' || depth > 8) return source;
        if (Object.hasOwn(catalog[language] || {}, source)) return t(source);
        for (const item of templates) {
            const match = source.match(item.regex);
            if (!match) continue;
            let index = 0;
            return (catalog[language]?.[item.key] || item.key).replace(/\{[^}]*\}/g, token => {
                const position = token === '{}' ? index++ : item.tokens.indexOf(token);
                return match[position+1] ?? token;
            });
        }
        const split = source.indexOf(': ');
        if (split >= 0) return message(source.slice(0,split),depth+1)+': '+message(source.slice(split+2),depth+1);
        if (source.includes('\n')) return source.split('\n').map(s=>message(s,depth+1)).join('\n');
        return source;
    }
    function captureStatic(root = document.documentElement) {
        // Capture once, before user filenames, tags and paths are inserted.
        if (!root) return;
        function visit(node) {
            if (node.nodeType === 3) {
                const source = node.nodeValue.trim();
                if (Object.hasOwn(catalog.en || {}, source)) records.push({node, source, before:node.nodeValue.match(/^\s*/)[0], after:node.nodeValue.match(/\s*$/)[0]});
            } else if (node.nodeType === 1) {
                if (['SCRIPT','STYLE','CODE'].includes(node.tagName) || node.hasAttribute('data-no-i18n')) return;
                for (const attr of ['title','aria-label','placeholder','alt']) {
                    const source = node.getAttribute(attr);
                    if (source && Object.hasOwn(catalog.en || {}, source)) records.push({node, attr, source});
                }
                for (const child of node.childNodes) visit(child);
            }
        }
        visit(root); renderStatic();
    }
    function renderStatic() {
        if (!document.documentElement) return;
        document.documentElement.lang = language;
        for (const r of records) {
            if (r.attr) r.node.setAttribute(r.attr, t(r.source));
            else r.node.nodeValue = r.before + t(r.source) + r.after;
        }
        document.querySelectorAll('[data-language-select]').forEach(el => el.value = language);
    }
    function set(value) {
        language = supported.includes(value) ? value : 'en';
        renderStatic();
    }
    window.vt = t;
    window.VaultI18n = {t, message, captureStatic, set, get language(){return language;}, get locale(){return {ko:'ko-KR',en:'en-US',ja:'ja-JP'}[language];}};
})();
