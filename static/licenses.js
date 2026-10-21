(() => {
    'use strict';
    let dialog, search, list, content, status, heading, explanation, closeButton, catalog, loading;
    const t=(s,...args)=>window.vt(s,...args);
    const scopes={project:'프로젝트',app:'앱 의존성',pdf:'PDF 자원',generated:'생성 코드 관련 고지',native:'네이티브 라이브러리'};
    function build(){
        if(dialog)return;
        const make=(tag,cls)=>{const el=document.createElement(tag);if(cls)el.className=cls;return el;};
        dialog=make('dialog','licenses-dialog');dialog.id='licenses-dialog';dialog.setAttribute('aria-labelledby','licenses-title');
        const header=make('div','licenses-header');heading=make('h2');heading.id='licenses-title';
        closeButton=make('button');closeButton.type='button';closeButton.addEventListener('click',()=>dialog.close());
        header.append(heading,closeButton);
        explanation=make('p','licenses-explanation');
        search=make('input');search.type='search';search.autocomplete='off';search.addEventListener('input',renderList);
        status=make('p','licenses-status');status.setAttribute('role','status');
        const body=make('div','licenses-body');list=make('div','licenses-list');content=make('pre','licenses-text');
        content.tabIndex=0;content.setAttribute('data-no-i18n','');body.append(list,content);
        dialog.append(header,explanation,search,status,body);
        dialog.addEventListener('keydown',event=>{
            event.stopPropagation();
            if(event.key==='Escape'){event.preventDefault();dialog.close();}
        });
        document.body.appendChild(dialog);
    }
    function show(component){
        const title=component.name+(component.version?' '+component.version:'');
        content.textContent=title+'\n'+component.license+'\n'+t(scopes[component.scope])+'\n\n'+
            component.notices.map(n=>n.file+'\n\n'+catalog.texts[n.text]).join('\n\n────────────────────\n\n');
        content.scrollTop=0;
        for(const button of list.children)button.setAttribute('aria-pressed',String(button.dataset.name===title));
    }
    function renderList(){
        if(!catalog)return;
        list.replaceChildren();content.textContent='';
        const query=search.value.trim().toLocaleLowerCase();
        const found=catalog.components.filter(c=>(c.name+' '+c.version+' '+c.license+' '+t(scopes[c.scope])).toLocaleLowerCase().includes(query));
        status.textContent=t('구성 요소 {0}개',found.length);
        for(const component of found){
            const button=document.createElement('button');button.type='button';
            button.dataset.name=component.name+(component.version?' '+component.version:'');
            button.textContent=button.dataset.name+' · '+component.license;
            button.addEventListener('click',()=>show(component));list.appendChild(button);
        }
        if(found.length)show(found[0]);
        else content.textContent=t('검색 결과가 없습니다.');
    }
    async function load(){
        if(catalog)return catalog;
        if(!loading)loading=(async()=>{
            const embedded=document.getElementById('embedded-licenses');
            if(embedded)return JSON.parse(embedded.textContent);
            const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),15000);
            try{
                const response=await fetch('licenses.json',{signal:controller.signal,cache:'no-cache'});
                if(!response.ok)throw new Error('License catalog unavailable');
                return await response.json();
            }finally{clearTimeout(timer);}
        })().then(data=>{
            if(!Array.isArray(data.components)||!data.texts||!data.components.length)throw new Error('Invalid license catalog');
            for(const c of data.components)for(const n of c.notices)if(typeof data.texts[n.text]!=='string')throw new Error('Missing license text');
            catalog=data;return data;
        }).finally(()=>{loading=null;});
        return loading;
    }
    async function open(){
        build();
        heading.textContent=t('정보·라이선스');closeButton.textContent=t('닫기');
        search.placeholder=t('구성 요소 또는 라이선스 검색');search.setAttribute('aria-label',search.placeholder);
        explanation.textContent=t('valtheum의 자체 코드는 MIT 라이선스입니다. 타사 구성 요소에는 각자의 라이선스가 적용됩니다. 아래 원문은 번역하지 않습니다.');
        if(!dialog.open)dialog.showModal();
        search.value='';search.disabled=true;status.textContent=t('불러오는 중…');list.replaceChildren();content.textContent='';
        try{
            await load();
            if(!dialog.open)return;
            heading.textContent='valtheum '+catalog.version+' · '+t('정보·라이선스');
            search.disabled=false;renderList();search.focus();
        }catch(_){
            if(dialog.open)status.textContent=t('라이선스를 불러오지 못했습니다. 창을 닫고 다시 열어 주세요.');
        }
    }
    window.VaultLicenses={open,isOpen:()=>!!dialog?.open};
    document.querySelectorAll('[data-open-licenses]').forEach(button=>button.addEventListener('click',open));
})();
