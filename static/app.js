document.addEventListener('DOMContentLoaded', () => {
    // DOM Elements
    const folderSelect = document.getElementById('folder-select');
    const btnAddFolder = document.getElementById('btn-add-folder');
    const btnCloseLibrary = document.getElementById('btn-close-library');
    const btnRescanFolder = document.getElementById('btn-rescan-folder');
    const rescanIcon = document.getElementById('rescan-icon');
    const btnResetAllWatch = document.getElementById('btn-reset-all-watch');
    const btnResetAllScores = document.getElementById('btn-reset-all-scores');
    const btnResetAllRatings = document.getElementById('btn-reset-all-ratings');
    const btnStartArena = document.getElementById('btn-start-arena');
    const btnOpenTrash = document.getElementById('btn-open-trash');
    const btnBackupDb = document.getElementById('btn-backup-db');
    const sidebarTrashCountEl = document.getElementById('sidebar-trash-count');
    const mediaListEl = document.getElementById('media-list');
    const mediaCountEl = document.getElementById('media-count');
    const searchInput = document.getElementById('search-input');
    const btnClearSearch = document.getElementById('btn-clear-search');
    const tagSearchInput = document.getElementById('tag-search-input');
    const btnClearTagSearch = document.getElementById('btn-clear-tag-search');
    let searchDebounceTimer = null;
    const sortSelect = document.getElementById('sort-select');
    const btnSortOrder = document.getElementById('btn-sort-order');
    const sortOrderIcon = document.getElementById('sort-order-icon');
    const sortOrderLabel = document.getElementById('sort-order-label');
    const scanStatusEl = document.getElementById('scan-status');
    const toolsAccordion = document.getElementById('tools-accordion');
    if (toolsAccordion && typeof localStorage !== 'undefined') {
        try {
            if (localStorage.getItem('vault_tools_accordion_open') === 'true') {
                toolsAccordion.open = true;
            }
            toolsAccordion.addEventListener('toggle', () => {
                try {
                    localStorage.setItem('vault_tools_accordion_open', toolsAccordion.open ? 'true' : 'false');
                } catch (_) {}
            });
        } catch (_) {}
    }
    
    // Stage Elements
    const videoStage = document.getElementById('video-player-stage');
    const imageStage = document.getElementById('image-viewer-stage');
    const arenaStage = document.getElementById('arena-stage');
    const btnExitArena = document.getElementById('btn-exit-arena');
    const trashStage = document.getElementById('trash-stage');
    const btnExitTrash = document.getElementById('btn-exit-trash');
    
    // Trash Stage Elements
    const tabTrashItems = document.getElementById('tab-trash-items');
    const tabTrashLogs = document.getElementById('tab-trash-logs');
    const trashItemsContent = document.getElementById('trash-tab-items-content');
    const trashLogsContent = document.getElementById('trash-tab-logs-content');
    const trashItemsGrid = document.getElementById('trash-items-grid');
    const trashLogsTbody = document.getElementById('trash-logs-tbody');
    const btnEmptyTrash = document.getElementById('btn-empty-trash');
    const btnClearTrashLogs = document.getElementById('btn-clear-trash-logs');
    const trashLogsStatus = document.getElementById('trash-logs-status');
    let trashLogsLoadVersion = 0;
    let trashItemsLoadVersion = 0, trashCountVersion = 0;
    let trashLogsLoaded = false;
    let deletingTrashLog = false;
    const trashCountEl = document.getElementById('trash-count');
    const logsCountEl = document.getElementById('logs-count');

    // Video Elements
    const videoWrapper = document.querySelector('.video-wrapper');
    const video = document.getElementById('main-video');
    const btnPlayPause = document.getElementById('btn-play-pause');
    const btnMute = document.getElementById('btn-mute');
    const volumeBar = document.getElementById('volume-bar');
    const seekBar = document.getElementById('seek-bar');
    const seekBarWrapper = document.getElementById('seek-bar-wrapper');
    const seekHoverIndicator = document.getElementById('seek-hover-indicator');
    const seekHoverTime = document.getElementById('seek-hover-time');
    const currentTimeEl = document.getElementById('current-time');
    const durationEl = document.getElementById('duration');
    const speedSelect = document.getElementById('speed-select');
    const btnCaptureThumb = document.getElementById('btn-capture-thumb');
    const btnTrashCurrentVideo = document.getElementById('btn-trash-current-video');
    const canvas = document.getElementById('thumbnail-canvas');
    const volumeHud = document.getElementById('video-volume-hud');
    const volumeHudIcon = document.getElementById('volume-hud-icon');
    const volumeHudText = document.getElementById('volume-hud-text');
    const volumeHudFill = document.getElementById('volume-hud-fill');

    // Image / Archive Viewer Elements
    const viewerImage = document.getElementById('viewer-image');
    const viewerImage2 = document.getElementById('viewer-image-2');
    const imageDisplayBox = document.getElementById('image-display-box');
    const imageWrapper = document.getElementById('image-wrapper');
    const btnImgPrev = document.getElementById('btn-img-prev');
    const btnImgNext = document.getElementById('btn-img-next');
    const imgEntryName = document.getElementById('img-entry-name');
    const imgPageCounter = document.getElementById('img-page-counter');
    const imageSeekBar = document.getElementById('image-seek-bar');
    const imageSeekWrapper = document.getElementById('image-seek-wrapper');
    const imageSeekHoverIndicator = document.getElementById('image-seek-hover-indicator');
    const imageSeekHoverTooltip = document.getElementById('image-seek-hover-tooltip');
    const btnCaptureImgThumb = document.getElementById('btn-capture-img-thumb');
    const btnTrashCurrentImage = document.getElementById('btn-trash-current-image');
    let currentImageStep = 1;
    let archiveLoadToken = 0;
    let imageWheelCooldown = false;
    const imageMetaCache = new Map();
    const imageMetaPending = new Map();
    const imageMetaQueue = [];
    let imageMetaActive = 0;
    let imagePrefetchActive = 0, imageLoadMs = null, imageLoadPixels = 0;
    const IMAGE_META_CONCURRENCY = 2;
    const IMAGE_META_CACHE_MAX = 128;

    // Arena Stage Elements
    const arenaCardA = document.getElementById('arena-card-a');
    const arenaCardB = document.getElementById('arena-card-b');
    const arenaThumbA = document.getElementById('arena-thumb-a');
    const arenaThumbB = document.getElementById('arena-thumb-b');
    const arenaThumbWrapperA = document.getElementById('arena-thumb-wrapper-a');
    const arenaThumbWrapperB = document.getElementById('arena-thumb-wrapper-b');
    const arenaVideoA = document.getElementById('arena-video-a');
    const arenaVideoB = document.getElementById('arena-video-b');
    const arenaFallbackA = document.getElementById('arena-fallback-a');
    const arenaFallbackB = document.getElementById('arena-fallback-b');
    const arenaPageA = document.getElementById('arena-page-a');
    const arenaPageB = document.getElementById('arena-page-b');
    const arenaTypeA = document.getElementById('arena-type-a');
    const arenaTypeB = document.getElementById('arena-type-b');
    const arenaTitleA = document.getElementById('arena-title-a');
    const arenaTitleB = document.getElementById('arena-title-b');
    const arenaScoreA = document.getElementById('arena-score-a');
    const arenaScoreB = document.getElementById('arena-score-b');
    const arenaDeltaA = document.getElementById('arena-delta-a');
    const arenaDeltaB = document.getElementById('arena-delta-b');
    const arenaPinnedBadge = document.getElementById('arena-pinned-badge');
    const arenaPinnedText = document.getElementById('arena-pinned-text');
    const btnUnpinArena = document.getElementById('btn-unpin-arena');
    const btnPinCardA = document.getElementById('btn-pin-card-a');
    const btnArenaPass = document.getElementById('btn-arena-pass');
    const btnArenaPassTop = document.getElementById('btn-arena-pass-top');
    let arenaPinnedMediaId = null;

    let watchSession = null;
    const pendingWatchRecords = new Set();
    const watchResets = new Set();
    let watchRetryTimer = null;
    let currentFile = null;
    let filesData = [];
    let inArenaMode = false;
    let inTrashMode = false;
    let currentArenaPair = null;
    let isVoting = false;
    let arenaLoadVersion = 0;
    let videoMetadataCleanup = null;
    let imageCaptureResetTimer = null;
    const imageCaptureDefaultText = btnCaptureImgThumb?.textContent || '';

    function resetImageCaptureButton() {
        if (imageCaptureResetTimer) { clearTimeout(imageCaptureResetTimer); imageCaptureResetTimer = null; }
        if (btnCaptureImgThumb) {
            btnCaptureImgThumb.textContent = imageCaptureDefaultText;
            btnCaptureImgThumb.style.background = '';
        }
    }

    // Sorting State
    let currentSortBy = 'score';
    let currentSortOrder = 'desc';
    const mediaVirtualList = new window.VirtualMediaList(mediaListEl, createMediaCard, decorateMediaCard);

    // All API/media requests are scoped to the current library session.  A
    // response from an earlier folder must never repaint the current folder.
    let vaultSession = { token: null, generation: null };
    let requestEpoch = 0;
    const activeRequests = new Set();
    const STALE_RESPONSE = Object.assign(new Error('Previous library request cancelled'), { name: 'AbortError' });
    let selectionVersion = 0;
    let folderTransition = false;
    let scanPollTimer = null;
    let scanPollingEpoch = null;
    const metaCancels = new Set();
    const captureScope = () => ({ epoch: requestEpoch, generation: vaultSession.generation });
    const isFresh = scope => scope.epoch === requestEpoch && scope.generation === vaultSession.generation;

    const mediaName = file => file.display_name || file.rel_path;
    const nameEditor = new window.MediaNameEditor({fetch:vaultFetch,captureScope,isFresh,
        onChanged:async scope=>{await loadMediaFiles();if(isFresh(scope)&&inArenaMode)await loadArenaPair();}});

    const pageEditor = new window.ArchivePageEditor({fetch:vaultFetch,captureScope,isFresh,mediaUrl,
        onManageOriginal:()=>archiveSecurity.open(),
        onChanged:async(scope,file,index)=>{
            const selection=selectionVersion,pageToken=archiveLoadToken,noteVersion=noteEditor.version;
            const canRefreshViewer=()=>isFresh(scope)&&currentFile?.id===file.id&&
                !inArenaMode&&!inTrashMode&&noteEditor.canAutoSelect()&&
                selectionVersion===selection&&archiveLoadToken===pageToken&&noteEditor.version===noteVersion;
            // Closing the editor lets the user move on while deletion finishes.
            // Refresh metadata without auto-selection; only reload an unchanged viewer.
            const refreshViewer=canRefreshViewer();
            await loadMediaFiles({selectFirst:false});
            if(refreshViewer&&canRefreshViewer()){
                await selectMedia(currentFile,{archivePage:index});
            }
        }});
    const deletePageButton=document.getElementById('btn-delete-archive-page');
    deletePageButton.addEventListener('click',()=>pageEditor.open(currentFile,archiveEntries,currentImageIdx,currentImageStep));

    const seriesEditor = new window.SeriesEditor({
        fetch: vaultFetch, captureScope, isFresh,
        onSaved: async scope => {
            await loadMediaFiles();
            if (isFresh(scope) && inArenaMode) await loadArenaPair();
        }
    });
    document.getElementById('btn-series').addEventListener('click', () => seriesEditor.open());

    async function refreshSecurityLibrary(scope, rescan = false) {
        await loadFolders();
        if (!isFresh(scope)) return null;
        await loadMediaFiles();
        if (!isFresh(scope)) return null;
        if (rescan && !privacy.covered && !dbSecurity.locked) {
            const status = await (await vaultFetch('/api/folders/rescan', {method:'POST', scope})).json();
            if (!isFresh(scope)) return null;
            renderScanStatus(status);
            if (status.running) pollScanStatus();
        }
        return scope;
    }
    async function resyncSecurityLibrary(scope) {
        await initializeVaultSession(scope);
        return refreshSecurityLibrary(captureScope());
    }
    const dbSecurity = new window.DatabaseSecurity({
        fetch: vaultFetch, captureScope, isFresh,
        onLocking:()=>{resetLibraryView();dbSecurity.dialog.close();privacy.lockUnconfirmed=true;privacy.setCovered(true);},
        onChanged: async payload => {
            adoptLibraryState(payload);
            return refreshSecurityLibrary(captureScope(), true);
        },
        onResync: resyncSecurityLibrary
    });

    const archiveSecurity = new window.ArchiveSecurity({fetch:vaultFetch, captureScope, isFresh,
        onChanged: async scope => {
            if(!isFresh(scope))return;
            await loadMediaFiles({selectFirst:false,bounded:true});
            if(isFresh(scope)&&inArenaMode)await loadArenaPair();
        }
    });

    const dbRestore = new window.BackupRestore({fetch:vaultFetch, captureScope, isFresh,
        onChanged: async payload => { adoptLibraryState(payload); return refreshSecurityLibrary(captureScope()); },
        onResync: resyncSecurityLibrary
    });

    const privacy = new window.PrivacyScreen({fetch:vaultFetch,mediaUrl:scopedUrl,captureScope,isFresh,
        onPrepareEntry:async fresh=>{
            if(!privacy.lockUnconfirmed&&!privacy.closeUnconfirmed)return;
            if(dbSecurity.pending||folderTransition)throw new Error('Concealment request pending');
            // Reconcile a lost response without adopting an unlocked library's UI.
            const session=await(await vaultFetch('/api/session?concealment=true')).json();
            if(!fresh())throw STALE_RESPONSE;
            if(session.generation!==vaultSession.generation){invalidateRequests();vaultSession.generation=session.generation;}
            const locked=session.covered===true?session:await(await vaultFetch(privacy.closeUnconfirmed?'/api/folders/close':'/api/security/lock',{method:'POST'})).json();
            if(!fresh())throw STALE_RESPONSE;
            if(locked.covered!==true)throw new Error('Lock not confirmed');
            if(locked.generation!==vaultSession.generation){invalidateRequests();vaultSession.generation=locked.generation;}
            privacy.lockUnconfirmed=false;privacy.closeUnconfirmed=false;
        },
        onCancelEntry:()=>changeLibrary('/api/folders/close'),
        onRevealed: async payload => {
            adoptLibraryState(payload);
            return refreshSecurityLibrary(captureScope(), true);
        },
        onResync: resyncSecurityLibrary
    });

    const integrityCheck = new window.IntegrityCheck({fetch:vaultFetch,captureScope,isFresh,
        canRun:()=>!privacy.covered&&!dbSecurity.locked&&!folderTransition});
    const videoRecovery = new Map();
    const storageRecovery = new window.StorageRecovery({fetch:vaultFetch,captureScope,isFresh,
        canRecover:()=>!privacy.covered&&!dbSecurity.locked&&!folderTransition,
        onRecovered:async scope=>{
            if (!isFresh(scope)) return;
            clearImageJobs();
            // Existing metadata stays visible, including when NAS reads still fail.
            applySearchAndRender();
            for (const img of document.querySelectorAll('img')) {
                if (!img.src) continue;
                const url = new URL(img.src, window.location.origin);
                if (url.origin !== window.location.origin || !url.pathname.startsWith('/api/') ||
                    url.searchParams.get('library') !== String(scope.generation)) continue;
                url.searchParams.set('retry', Date.now()); img.src = url.href;
            }
            if (currentFile?.file_type === 'PDF' && !inArenaMode && !inTrashMode) {
                await pdfViewer.open(currentFile, mediaUrl('/api/media/'+encodeMediaPath(currentFile.rel_path)), pdfViewer.page);
                if (!isFresh(scope)) return;
            }
            for (const player of [video, arenaVideoA, arenaVideoB]) {
                if (!player.src) continue;
                const url = new URL(player.src, window.location.origin);
                if (url.origin !== window.location.origin || !url.pathname.startsWith('/api/media/') ||
                    url.searchParams.get('library') !== String(scope.generation)) continue;
                const src = player.src;
                const selection = selectionVersion;
                const previous = videoRecovery.get(player);
                if (previous) player.removeEventListener('loadedmetadata', previous.restore);
                const same = previous?.src === src && previous.selection === selection;
                const position = same ? previous.position : player.currentTime || 0;
                const paused = same ? previous.paused : player.paused;
                const restore = () => {
                    player.removeEventListener('loadedmetadata', restore);
                    videoRecovery.delete(player);
                    if (!isFresh(scope) || selection !== selectionVersion || player.src !== src) return;
                    if (Number.isFinite(player.duration)) player.currentTime = Math.min(position, player.duration);
                    if (!paused) player.play().catch(()=>{});
                };
                videoRecovery.set(player, {restore, src, selection, position, paused});
                player.addEventListener('loadedmetadata', restore, {once:true});
                player.load();
            }
        }
    });
    document.addEventListener('error', event => {
        const target=event.target;
        if (!target || !['IMG','VIDEO'].includes(target.tagName)) return;
        try {
            const url=new URL(target.currentSrc || target.src,window.location.origin);
            if (url.origin===window.location.origin && url.pathname.startsWith('/api/') &&
                url.searchParams.get('library')===String(vaultSession.generation)) storageRecovery.note();
        } catch (_) {}
    }, true);

    function invalidateRequests() {
        bookmarkPreparationCancel?.();
        bookmarkMutations.reset();
        clearTimeout(watchRetryTimer); watchRetryTimer = null;
        pendingWatchRecords.clear();
        watchResets.clear();
        integrityCheck.reset();
        storageRecovery.reset();
        for (const [player, pending] of videoRecovery) player.removeEventListener('loadedmetadata', pending.restore);
        videoRecovery.clear();
        requestEpoch += 1;
        activeRequests.forEach(controller => controller.abort());
        activeRequests.clear();
        clearTimeout(scanPollTimer);
        scanPollTimer = null;
        scanPollingEpoch = null;
    }

    function scopedUrl(input, extra = {}) {
        const url = new URL(input, window.location.origin);
        if (vaultSession.token) url.searchParams.set('session', vaultSession.token);
        if (vaultSession.generation != null) url.searchParams.set('library', vaultSession.generation);
        Object.entries(extra).forEach(([key, value]) => url.searchParams.set(key, value));
        return `${url.pathname}${url.search}${url.hash}`;
    }

    async function vaultFetch(input, options = {}) {
        const { scope = captureScope(), signal: externalSignal, reportError = true, discardBody = false, ...fetchOptions } = options;
        const ensureFresh = () => { if (!isFresh(scope)) throw STALE_RESPONSE; };
        ensureFresh();
        const controller = new AbortController();
        activeRequests.add(controller);
        const release = () => {activeRequests.delete(controller);externalSignal?.removeEventListener('abort',abort);};
        const abort = () => controller.abort();
        controller.signal.addEventListener('abort',release,{once:true});
        if (externalSignal?.aborted) controller.abort();
        else externalSignal?.addEventListener('abort', abort, {once:true});
        const headers = new Headers(fetchOptions.headers || {});
        if (vaultSession.token) headers.set('X-Vault-Token', vaultSession.token);
        let returnedResponse=false;
        try {
            const response = await fetch(scopedUrl(input), { ...fetchOptions, headers, signal: controller.signal });
            ensureFresh();
            for (const method of ['json', 'text', 'blob', 'arrayBuffer', 'formData']) {
                if(typeof response[method]!=='function')continue;
                const original = response[method].bind(response);
                response[method] = async () => {
                    try {
                        ensureFresh();
                        if(controller.signal.aborted)throw STALE_RESPONSE;
                        const value = await original();ensureFresh();
                        if(controller.signal.aborted)throw STALE_RESPONSE;
                        return value;
                    } finally { release(); }
                };
            }
            if (!response.ok) {
                const message = await response.text();
                const error = new Error(window.VaultI18n.message(message) || window.vt("요청 실패 ({0})", response.status));
                error.status = response.status;
                if (reportError && scanStatusEl) scanStatusEl.textContent = error.message;
                throw error;
            }
            if(discardBody){
                // This call only needs the status. Cancel the unused body instead
                // of retaining a controller indefinitely or buffering its data.
                response.body?.cancel().catch(()=>{});
                controller.abort();
            }else if(response.body===null||response.headers?.get('content-length')==='0')release();
            returnedResponse=true;
            return response;
        } catch (error) {
            if (isFresh(scope) && (!fetchOptions.method || fetchOptions.method === 'GET') &&
                (error.status >= 500 || error.name === 'TypeError')) storageRecovery.note();
            throw error;
        } finally {
            // Headers are not completion: library changes must still cancel an
            // in-flight body. Body readers, explicit discard, and abort release it.
            if(!returnedResponse)release();
        }
    }

    function mediaUrl(path, extra = {}) { return scopedUrl(path, extra); }

    function clearImageJobs() {
        imageLoadMs = null; imageLoadPixels = 0;
        for (const job of imageMetaQueue.splice(0)) {
            imageMetaPending.delete(job.key);
            job.resolve({ isPortrait: false, width: 0, height: 0 });
        }
        for (const cancel of [...metaCancels]) cancel();
        imageMetaCache.clear();
    }

    const noteEditor = new window.NoteEditor({fetch:vaultFetch,captureScope,isFresh,
        // These operations can replace the library even after their dialog closes.
        canEdit:()=>!dbSecurity.pending&&!dbRestore.pending&&!folderTransition,
        onChanged:async(scope,id,{reveal=true}={})=>{
            const editorVersion=noteEditor.version,selection=selectionVersion;
            // Refresh metadata without selecting anything while a newer draft may be open.
            await loadMediaFiles({selectFirst:false,bounded:true});
            if(!reveal||!isFresh(scope)||!noteEditor.isOpen()||noteEditor.version!==editorVersion||selectionVersion!==selection)return;
            const file=filesData.find(f=>f.id===id);
            if(file){
                if(!getFilteredFiles().some(f=>f.id===id)){mediaTypeFilter.value='NOTE';clearSearchFilters();}
                if(inArenaMode)exitArena();if(inTrashMode)exitTrash();
                await selectMedia(file);
            }else{
                // Saving succeeded; the acknowledged ID remains readable independently
                // of a failed or superseded list refresh. View also retries a failed read.
                await noteEditor.read(false);
            }
        },
        onSetupEncryption:()=>dbSecurity.open(),confirmEncryption:()=>confirm(window.vt('메모는 현재 라이브러리의 키와 비밀번호로 암호화됩니다. 복원하려면 vault.key와 비밀번호가 모두 필요합니다. DB 백업과 별도로 .vtn 메모 파일도 백업해 주세요. 저장할까요?'))});
    document.getElementById('btn-new-note').addEventListener('click',()=>{video.pause();noteEditor.openNew();});
    document.getElementById('btn-open-note').addEventListener('click',()=>noteEditor.open(currentFile));
    const pdfViewer = new window.PdfViewer({fetch:vaultFetch, mediaUrl, captureScope, isFresh,
        onThumbnail:updateThumbnailState,
        onPage:index => {
            const session = watchSession;
            if (!isWatching(session) || session.type !== 'PDF') return;
            session.pages.add(index); qualifyWatch(session);
        }
    });

    function resetLibraryView() {
        resetBackupUi();
        metadataStatus.textContent='';metadataState.hidden=true;metadataCheck.disabled=false;metadataCheckVersion++;
        clearSearchFilters();
        noteEditor.reset();
        document.getElementById('note-stage').hidden=true;
        document.getElementById('note-stage-title').textContent='';
        pdfViewer.reset(true);
        watchSession = null;
        watchDisplayWrites.clear();
        seriesEditor.reset();
        nameEditor.reset();
        pageEditor.reset();
        resetBookmarks();
        trashLogsLoadVersion += 1;
        trashLogsLoaded = false;
        deletingTrashLog = false;
        logsCountEl.textContent = '0';
        trashLogsStatus.textContent = '';
        document.getElementById('trash-items-status').textContent='';
        trashMutation=null;trashItemsLoadVersion++;trashCountVersion++;updateTrashItemControls();
        updateTrashLogControls();
        invalidateRequests();
        selectionVersion += 1;
        archiveLoadToken += 1;
        clearImageJobs();
        currentFile = null; filesData = []; setArchiveEntries([]);
        currentArenaPair = null; arenaPinnedMediaId = null; setArenaBusy(false);
        arenaLoadVersion += 1;
        if (videoMetadataCleanup) { videoMetadataCleanup(); videoMetadataCleanup = null; }
        resetImageCaptureButton();
        inArenaMode = false; inTrashMode = false;
        mediaVirtualList.reset();
        video.pause(); video.removeAttribute('src'); video.load();
        viewerImage.removeAttribute('src'); viewerImage2?.removeAttribute('src');
        stopArenaVideos(); resetArenaArchiveStates();
        arenaStage.style.display = 'none'; trashStage.style.display = 'none';
        imageStage.style.display = 'none'; videoStage.style.display = 'none';
        trashItemsGrid.replaceChildren(); trashLogsTbody.replaceChildren();
        arenaThumbA.removeAttribute('src'); arenaThumbB.removeAttribute('src');
        arenaTitleA.textContent = ''; arenaTitleB.textContent = '';
        arenaTitleA.removeAttribute('title'); arenaTitleB.removeAttribute('title');
        arenaScoreA.textContent = ''; arenaScoreB.textContent = '';
        arenaPageA.textContent = ''; arenaPageB.textContent = '';
        dbSecurity.clear();
        dbSecurity.dialog.close();
        dbRestore.reset();
        archiveSecurity.reset();
        document.getElementById('media-count').textContent = window.vt("0개");
        document.getElementById('sidebar-trash-count').textContent = '0';
    }

    function renderScanStatus(status) {
        if (!status || !scanStatusEl) return;
        const errors = status.report?.errors || [];
        scanStatusEl.textContent = status.running ? window.vt("동기화 중… {0}개", status.scanned_count || 0) :
            window.VaultI18n.message(status.error) ? window.vt("동기화 오류: {0}", window.VaultI18n.message(status.error)) : errors.length ?
            window.vt("일부 경로를 읽지 못해 기존 기록을 보존했습니다.\n{0}", errors.join('\n')) :
            status.report ? window.vt("동기화 완료: {0}개, 변경 {1}개{2}", status.report.scanned_count, status.report.new_or_updated, status.report.missing_count ? window.vt(" · 파일 없음 {0}개 (기록 보존)", status.report.missing_count) : '') : '';
        scanStatusEl.title = scanStatusEl.textContent;
        btnRescanFolder.disabled = !!status.running;
        rescanIcon?.classList.toggle('spin-animation', !!status.running);
    }

    function adoptLibraryState(payload) {
        if (payload?.covered === true) {
            privacy.lockUnconfirmed=false;privacy.closeUnconfirmed=false;
            resetLibraryView();
            if(payload.generation!==undefined)vaultSession.generation=payload.generation;
            folderSelect.replaceChildren();
            scanStatusEl.textContent='';
            dbSecurity.dismiss();dbRestore.reset();
            document.getElementById('shortcut-dialog').close();
            privacy.setCovered(true);
            return privacy.loadGallery();
        }
        if(payload?.covered === false)privacy.setCovered(false);
        if (payload?.generation !== undefined && payload.generation !== vaultSession.generation) {
            resetLibraryView();
            vaultSession.generation = payload.generation;
        }
        renderScanStatus(payload?.scan);
        if (payload?.security?.locked) {
            if (!dbSecurity.locked) resetLibraryView();
            clearSearchFilters();
        }
        dbSecurity.update(payload?.security);
        if (payload?.scan?.running && !dbSecurity.locked) pollScanStatus();
    }

    async function pollScanStatus() {
        const scope = captureScope();
        if (scanPollingEpoch === scope.epoch) return;
        scanPollingEpoch = scope.epoch;
        const poll = async () => {
            if (!isFresh(scope)) return;
            try {
                const state = await (await vaultFetch('/api/scan', { scope })).json();
                renderScanStatus(state);
                if (state.running) scanPollTimer = setTimeout(poll, 500);
                else { scanPollingEpoch = null; await loadMediaFiles(); }
            } catch (err) {
                if (isFresh(scope)) { scanPollingEpoch = null; renderScanStatus({ error: err.message }); }
            }
        };
        await poll();
    }

    async function changeLibrary(endpoint, payload) {
        if (folderTransition) return;
        const closing=endpoint==='/api/folders/close';
        folderTransition = true;
        document.body.classList.add('library-changing');
        resetLibraryView();
        if(closing){privacy.closeUnconfirmed=true;privacy.setCovered(true);}
        try {
            const options = { method: 'POST' };
            if (payload) { options.headers = { 'Content-Type': 'application/json' }; options.body = JSON.stringify(payload); }
            const config = await (await vaultFetch(endpoint, options)).json();
            await adoptLibraryState(config);
            await loadFolders();
            await loadMediaFiles();
        } catch (err) {
            // Never re-adopt an unlocked session after a failed or lost close response.
            // The hidden entry reconciles and retries the original close action.
            if(closing)return;
            const failureMessage = `${endpoint === '/api/folders/close' ? window.vt("라이브러리 닫기 실패") : window.vt("폴더 변경 실패")}: ${err.message}`;
            if (scanStatusEl) scanStatusEl.textContent = failureMessage;
            // Resync if the server switched before a connection was interrupted.
            await initializeVaultSession();
            await loadFolders();
            await loadMediaFiles();
            if (!privacy.covered && scanStatusEl) scanStatusEl.textContent = failureMessage;
        } finally {
            folderTransition = false;
            document.body.classList.remove('library-changing');
        }
    }

    // Archive / Image Viewer State
    let archiveEntries = [];
    let currentImageIdx = 0;
    let archiveBackStack = [];
    const archiveNameOrder = new Intl.Collator(undefined, {numeric:true});
    let archiveOrder = buildArchiveOrder([]);

    function buildArchiveOrder(entries) {
        // The API/storage order stays stable for existing bookmarks and thumbnails.
        // Only display order changes; translate positions at every save/jump boundary.
        const ordered = entries.map((name, storedIndex) => ({name, storedIndex}));
        ordered.sort((a,b) => archiveNameOrder.compare(a.name,b.name) || a.storedIndex-b.storedIndex);
        const storedToDisplay = [];
        ordered.forEach((item,index) => { storedToDisplay[item.storedIndex] = index; });
        return {entries:ordered.map(item=>item.name), storedToDisplay,
            displayToStored:ordered.map(item=>item.storedIndex)};
    }

    function setArchiveEntries(entries) {
        archiveBackStack = [];
        archiveOrder = buildArchiveOrder(entries);
        archiveEntries = archiveOrder.entries;
        deletePageButton.hidden = currentFile?.file_type !== 'ARCHIVE' || !currentFile?.encrypted;
        deletePageButton.disabled = archiveEntries.length < 2;
    }

    const bookmarkStrips = [document.getElementById('video-bookmarks'), document.getElementById('image-bookmarks')];
    const bookmarkStatuses = [document.getElementById('video-bookmark-status'), document.getElementById('image-bookmark-status')];
    const bookmarkRetries = [document.getElementById('video-bookmark-retry'), document.getElementById('image-bookmark-retry')];
    let bookmarkReadCancel = null;
    let bookmarkPreparationCancel = null;
    const bookmarkResults = ['', ''];
    const bookmarkButtons = [document.getElementById('btn-save-video-bookmark'), document.getElementById('btn-save-image-bookmark')];
    let bookmarkLoadVersion = 0;
    let bookmarkSaveJob = null;
    const bookmarkMutations = new window.BookmarkMutations({fetch:vaultFetch,captureScope,isFresh,onChange:updateBookmarkControls});
    function updateBookmarkControls() {
        const busy=!!bookmarkSaveJob||bookmarkMutations.busy(currentFile?.id);
        bookmarkButtons.forEach(button=>{button.disabled=busy;});
        bookmarkStrips.forEach(strip=>{for(const card of strip.children)card.children[1].disabled=busy;});
    }

    function resetBookmarks() {
        bookmarkPreparationCancel?.();
        bookmarkLoadVersion++;
        bookmarkReadCancel?.(); bookmarkReadCancel = null;
        bookmarkResults.fill('');
        bookmarkRetries.forEach(el => { el.hidden = true; el.disabled = false; });
        bookmarkSaveJob = null;
        bookmarkStrips.forEach(el => el.replaceChildren());
        bookmarkStatuses.forEach(el => el.textContent = '');
        bookmarkButtons.forEach(el => { el.disabled = false; el.hidden = true; });
    }

    function bookmarkContext() {
        const file = currentFile, scope = captureScope(), selection = selectionVersion;
        return { file, scope, fresh: () => isFresh(scope) && selection === selectionVersion && currentFile?.id === file?.id };
    }

    function bookmarkLabel(file, position) {
        return file.file_type === 'VIDEO' ? formatTime(position) :
            (archiveOrder.storedToDisplay[position] === undefined ? window.vt("없는 페이지") : window.vt("{0}페이지", archiveOrder.storedToDisplay[position] + 1));
    }

    async function loadBookmarks(context = bookmarkContext()) {
        const { file, scope, fresh } = context;
        if (!file || !['VIDEO', 'ARCHIVE'].includes(file.file_type) || archiveUnsupported(file)) return;
        const index = file.file_type === 'VIDEO' ? 0 : 1;
        const version = ++bookmarkLoadVersion;
        bookmarkReadCancel?.();
        const controller = new AbortController();
        let cancel;
        const stopped = new Promise((_, reject) => { cancel = () => { controller.abort(); reject(new Error(window.vt("저장 목록 응답을 받지 못했습니다."))); }; });
        bookmarkReadCancel = cancel;
        let timer;
        const current = () => fresh() && version === bookmarkLoadVersion;
        bookmarkButtons[index].hidden = false;
        if(bookmarkMutations.busy(file.id))bookmarkResults[index]=bookmarkMutations.uncertainMessage();
        updateBookmarkControls();
        bookmarkRetries[index].disabled = true;
        try {
            const request = (async () => {
                await bookmarkMutations.beforeRead(file.id,controller.signal);
                if(controller.signal.aborted)throw STALE_RESPONSE;
                timer=setTimeout(cancel,10000);
                return (await vaultFetch(`/api/files/${file.id}/bookmarks`, { scope, signal:controller.signal })).json();
            })();
            const bookmarks = await Promise.race([request, stopped]);
            if (!current()) return false;
            const result=bookmarkMutations.observe(file.id,bookmarks);
            if(result)bookmarkResults[index]=result;
            else if(!bookmarkMutations.busy(file.id)&&bookmarkResults[index]===bookmarkMutations.uncertainMessage())bookmarkResults[index]='';
            const cards = bookmarks.map(bookmark => {
                const card = document.createElement('div'); card.className = 'bookmark-card';
                const jump = document.createElement('button'); jump.className = 'bookmark-jump';
                const label = bookmarkLabel(file, bookmark.position);
                jump.title = window.vt("{0}로 이동", label);
                const img = document.createElement('img');
                img.src = mediaUrl(bookmark.thumbnail_url, { v: Date.now() });
                img.alt = window.vt("{0} 저장 화면", label); img.loading = 'lazy'; img.decoding = 'async';
                const caption = document.createElement('span'); caption.textContent = label;
                jump.append(img, caption);
                jump.onclick = () => {
                    if (!fresh()) return;
                    if (index === 0) {
                        if (!Number.isFinite(video.duration)) return;
                        video.currentTime = Math.min(bookmark.position, video.duration);
                    } else if (archiveOrder.storedToDisplay[bookmark.position] !== undefined) {
                        showArchiveImage(encodeMediaPath(file.rel_path), archiveOrder.storedToDisplay[bookmark.position]);
                    } else bookmarkStatuses[index].textContent = window.vt("저장한 페이지가 현재 압축파일에 없습니다.");
                };
                const remove = document.createElement('button'); remove.className = 'bookmark-delete';
                remove.textContent = '×'; remove.title = window.vt("{0} 저장 위치 삭제", label);
                remove.setAttribute('aria-label', remove.title);
                remove.onclick = async () => {
                    if (!fresh() || bookmarkSaveJob || bookmarkMutations.busy(file.id)) return;
                    remove.disabled = true;
                    try {
                        const applied=await bookmarkMutations.send(file.id,{bookmarkId:bookmark.id});
                        if (fresh()) {
                            if(applied)card.remove();
                            bookmarkResults[index] = applied?window.vt("저장 위치를 삭제했습니다."):bookmarkMutations.uncertainMessage();
                            bookmarkStatuses[index].textContent=bookmarkResults[index];
                            await loadBookmarks(context);
                        }
                    } catch (err) { if (fresh()) { bookmarkStatuses[index].textContent = window.vt("삭제 실패: {0}", err.message); remove.disabled = false; } }
                };
                card.append(jump, remove); return card;
            });
            bookmarkStrips[index].replaceChildren(...cards);
            bookmarkStatuses[index].textContent = bookmarkResults[index];
            bookmarkRetries[index].hidden = true;
            return true;
        } catch (err) {
            if (current()) {
                bookmarkStatuses[index].textContent = [bookmarkResults[index], window.vt("저장 위치를 불러오지 못했습니다: {0}", err.message)].filter(Boolean).join(' ');
                bookmarkRetries[index].hidden = false;
            }
            return false;
        } finally {
            clearTimeout(timer);
            if (bookmarkReadCancel === cancel) bookmarkReadCancel = null;
            if (current()) bookmarkRetries[index].disabled = false;
            if (current()) updateBookmarkControls();
        }
    }
    bookmarkRetries.forEach(button => button.addEventListener('click', () => loadBookmarks()));

    async function prepareBookmarkImages(images) {
        let cancel;
        const stopped = new Promise((_, reject) => { cancel = (reason = STALE_RESPONSE) => reject(reason); });
        bookmarkPreparationCancel = cancel;
        const timer = setTimeout(() => cancel(new Error(window.vt("이미지를 준비하는 데 시간이 너무 오래 걸립니다. 이미지가 표시된 후 다시 저장해 주세요."))), 10000);
        try {
            // decode() cannot be aborted; stop waiting and ignore its eventual result.
            await Promise.race([Promise.all(images.map(img => img.decode())), stopped]);
        } finally {
            clearTimeout(timer);
            if (bookmarkPreparationCancel === cancel) bookmarkPreparationCancel = null;
        }
    }

    async function saveBookmark() {
        const context = bookmarkContext();
        const { file } = context;
        if (!file || bookmarkSaveJob || bookmarkMutations.busy(file.id) || !['VIDEO', 'ARCHIVE'].includes(file.file_type) || archiveUnsupported(file)) return;
        const saving = {};
        const fresh = () => context.fresh() && bookmarkSaveJob === saving;
        const index = file.file_type === 'VIDEO' ? 0 : 1;
        const pageToken = archiveLoadToken;
        const position = index === 0 ? video.currentTime : archiveOrder.displayToStored[currentImageIdx];
        bookmarkSaveJob = saving; updateBookmarkControls();
        bookmarkResults[index] = '';
        bookmarkStatuses[index].textContent = window.vt("화면 저장 중…");
        try {
            let sources;
            if (index === 0) {
                if (video.readyState < 2 || video.seeking || !video.videoWidth || !Number.isFinite(position)) throw new Error(window.vt("영상 화면이 준비된 후 저장해 주세요."));
                sources = [{ element: video, width: video.videoWidth, height: video.videoHeight }];
            } else {
                if (!archiveEntries.length) throw new Error(window.vt("페이지를 먼저 열어 주세요."));
                const images = currentImageStep === 2 ? [viewerImage, viewerImage2] : [viewerImage];
                await prepareBookmarkImages(images);
                if (!fresh()) return;
                if (pageToken !== archiveLoadToken) throw new Error(window.vt("페이지가 바뀌었습니다. 다시 저장해 주세요."));
                sources = images.map(element => ({ element, width: element.naturalWidth, height: element.naturalHeight }));
            }
            // A fixed small canvas bounds storage even for very tall archive pages.
            const shot = document.createElement('canvas'); shot.width = 320; shot.height = 180;
            const ctx = shot.getContext('2d'); ctx.fillStyle = '#111'; ctx.fillRect(0, 0, 320, 180);
            const cell = 320 / sources.length;
            sources.forEach(({ element, width, height }, i) => {
                if (!width || !height) throw new Error(window.vt("화면을 읽을 수 없습니다."));
                const scale = Math.min(cell / width, 180 / height);
                ctx.drawImage(element, i * cell + (cell - width * scale) / 2, (180 - height * scale) / 2, width * scale, height * scale);
            });
            const thumbnail_data = shot.toDataURL('image/jpeg', 0.75).split(',')[1];
            if (!fresh()) return;
            const applied=await bookmarkMutations.send(file.id,{position,thumbnail_data});
            if (!fresh()) return;
            bookmarkResults[index] = applied?window.vt("{0} 저장됨", bookmarkLabel(file, position)):bookmarkMutations.uncertainMessage();
            bookmarkStatuses[index].textContent=bookmarkResults[index];
            await loadBookmarks(context);
        } catch (err) { if (fresh()) bookmarkStatuses[index].textContent = window.vt("저장 실패: {0}", err.message); }
        finally { if (bookmarkSaveJob === saving) { bookmarkSaveJob = null; updateBookmarkControls(); } }
    }
    bookmarkButtons.forEach(button => button.addEventListener('click', saveBookmark));

    // 1. Folder Management
    async function loadFolders() {
        if(privacy.covered)return;
        try {
            const res = await vaultFetch('/api/folders');
            const config = await res.json();

            folderSelect.innerHTML = '';
            config.folders.forEach(f => {
                const opt = document.createElement('option');
                opt.value = f.path;
                opt.textContent = `📁 ${f.name} (${f.path})`;
                opt.title = f.path;
                if (config.active_folder === f.path) {
                    opt.selected = true;
                }
                folderSelect.appendChild(opt);
            });
        } catch (err) {
            console.error('Error loading saved folders:', err);
        }
    }

    let folderSwitchChain = Promise.resolve();
    folderSelect.addEventListener('change', () => {
        const selectedPath = folderSelect.value;
        if (selectedPath) folderSwitchChain = folderSwitchChain.then(() => switchFolder(selectedPath));
    });

    btnAddFolder.addEventListener('click', () => changeLibrary('/api/folders/add'));

    btnCloseLibrary?.addEventListener('click', () => {
        if (folderTransition || privacy.covered) return;
        return changeLibrary('/api/folders/close');
    });

    let backupAttempt=null;
    function resetBackupUi(){
        const attempt=backupAttempt;if(!attempt)return;
        clearTimeout(attempt.timer);backupAttempt=null;
        btnBackupDb.textContent=attempt.label;
        btnBackupDb.style.background='';btnBackupDb.style.color='';
        btnBackupDb.disabled=dbSecurity.locked;
    }
    if (btnBackupDb) {
        btnBackupDb.addEventListener('click', async () => {
            if(backupAttempt||privacy.covered||dbSecurity.locked)return;
            const scope=captureScope(),attempt={label:btnBackupDb.textContent};
            backupAttempt=attempt;
            const fresh=()=>backupAttempt===attempt&&isFresh(scope)&&!privacy.covered&&!dbSecurity.locked;
            btnBackupDb.disabled=true;
            btnBackupDb.textContent = window.vt("⏳ 백업 중...");
            try {
                const res = await vaultFetch('/api/backup', { method: 'POST',scope });
                const result = await res.json();
                if(!fresh())return;
                if (result.status === 'success') {
                    btnBackupDb.textContent = window.vt("✅ 백업 완료!");
                    btnBackupDb.style.background = '#10b981';
                    btnBackupDb.style.color = '#fff';
                    alert(window.vt("✅ DB 백업 묶음이 생성되었습니다.\n{0}\n미디어 파일 자체는 포함되지 않습니다. 별도로 백업해 주세요.\n\n저장 경로:\n{1}", result.key_included ? window.vt("vault.key가 함께 포함되었습니다.") : window.vt("아직 생성된 미디어 키가 없어 DB만 포함했습니다."), result.backup_path));
                } else {
                    resetBackupUi();
                }
            } catch (err) {
                if(!fresh())return;
                console.error('Error backing up database:', err);
                alert(window.vt("DB 백업 중 오류가 발생했습니다."));
            } finally {
                if(fresh())attempt.timer=setTimeout(()=>{if(fresh())resetBackupUi();},2000);
            }
        });
    }

    btnRescanFolder?.addEventListener('click', async () => {
        const scope = captureScope();
        try {
            const status = await (await vaultFetch('/api/folders/rescan', { method: 'POST', scope })).json();
            if (!isFresh(scope)) return;
            renderScanStatus(status);
            if (status.running) await pollScanStatus();
        } catch (err) { if (isFresh(scope)) renderScanStatus({ error: err.message }); }
    });

    // 1-2. Reset All Watch Counts in Folder Handler
    if (btnResetAllWatch) {
        btnResetAllWatch.addEventListener('click', async () => {
            await runFolderReset({
                button: btnResetAllWatch,
                endpoint: '/api/folders/reset_watch',
                confirmText: window.vt("현재 폴더의 모든 미디어 시청 횟수 및 이력을 초기화하시겠습니까?"),
                errorText: window.vt("시청 횟수 초기화 중 오류가 발생했습니다."),
                updates: { watch_count: 0, last_watched_at: null }
            });
        });
    }

    // 1-3. Reset All Scores in Folder Handler
    if (btnResetAllScores) {
        btnResetAllScores.addEventListener('click', async () => {
            await runFolderReset({
                button: btnResetAllScores,
                endpoint: '/api/folders/reset_scores',
                confirmText: window.vt("⚠️ [경고] 현재 폴더에 있는 모든 미디어의 아레나 점수(Elo 레이팅)를 기본값(1000점)으로 초기화하시겠습니까?\n\n※ 이 작업은 되돌릴 수 없습니다."),
                errorText: window.vt("점수 초기화 중 오류가 발생했습니다."),
                updates: { score: 1000 }
            });
        });
    }

    // 1-4. Reset All Ratings in Folder Handler
    if (btnResetAllRatings) {
        btnResetAllRatings.addEventListener('click', async () => {
            await runFolderReset({
                button: btnResetAllRatings,
                endpoint: '/api/folders/reset_ratings',
                confirmText: window.vt("⚠️ [경고] 현재 폴더에 있는 모든 미디어의 별점을 초기화(0점)하시겠습니까?\n\n※ 이 작업은 되돌릴 수 없습니다."),
                errorText: window.vt("별점 초기화 중 오류가 발생했습니다."),
                updates: { rating: 0 }
            });
        });
    }

    async function switchFolder(path) { await changeLibrary('/api/folders/switch', { path }); }

    // 2. Sorting handlers
    sortSelect.addEventListener('change', () => {
        currentSortBy = sortSelect.value;
        loadMediaFiles({ resetScroll: true });
    });

    btnSortOrder.addEventListener('click', () => {
        currentSortOrder = currentSortOrder === 'desc' ? 'asc' : 'desc';
        if (currentSortOrder === 'asc') {
            sortOrderIcon.textContent = '▲';
            sortOrderLabel.textContent = window.vt("오름차순");
        } else {
            sortOrderIcon.textContent = '▼';
            sortOrderLabel.textContent = window.vt("내림차순");
        }
        loadMediaFiles({ resetScroll: true });
    });

    function formatDateTime(timestamp) {
        if (!timestamp) return window.vt("알 수 없음");
        const date = new Date(timestamp * 1000);
        const y = date.getFullYear();
        const m = String(date.getMonth() + 1).padStart(2, '0');
        const d = String(date.getDate()).padStart(2, '0');
        const hh = String(date.getHours()).padStart(2, '0');
        const mm = String(date.getMinutes()).padStart(2, '0');
        const ss = String(date.getSeconds()).padStart(2, '0');
        return `${y}-${m}-${d} ${hh}:${mm}:${ss}`;
    }

    function archiveUnsupported(file) { return file.file_type === "ARCHIVE" && /\.(rar|7z)$/i.test(file.rel_path); }

    function encodeMediaPath(relPath) {
        return relPath.split('/').map(encodeURIComponent).join('/');
    }

    function getPointerProgress(wrapper, clientX) {
        const rect = wrapper.getBoundingClientRect();
        const x = Math.max(0, Math.min(rect.width, clientX - rect.left));
        return {
            x,
            ratio: rect.width ? x / rect.width : 0
        };
    }

    function showHoverMarker(indicatorEl, tooltipEl, x, text) {
        if (indicatorEl) {
            indicatorEl.style.display = 'block';
            indicatorEl.style.left = `${x}px`;
        }
        if (tooltipEl) {
            tooltipEl.style.display = 'block';
            tooltipEl.style.left = `${x}px`;
            tooltipEl.textContent = text;
        }
    }

    function hideHoverMarker(indicatorEl, tooltipEl) {
        if (indicatorEl) indicatorEl.style.display = 'none';
        if (tooltipEl) tooltipEl.style.display = 'none';
    }

    function setArchiveDisplayMode(isDual) {
        if (viewerImage2) {
            viewerImage2.style.display = isDual ? 'block' : 'none';
            if (!isDual) viewerImage2.src = '';
        }
        if (imageDisplayBox) {
            imageDisplayBox.classList.toggle('dual', isDual);
            imageDisplayBox.classList.toggle('single', !isDual);
        }
    }

    function syncArchiveSeekBar(idx) {
        if (!imageSeekBar) return;
        const maxValue = Math.max(1, archiveEntries.length);
        imageSeekBar.max = maxValue;
        imageSeekBar.value = Math.max(1, Math.min(maxValue, idx + 1));
    }

    function updateThumbnailState(fileId, dataUrl, updates = {}) {
        if (currentFile && currentFile.id === fileId) {
            Object.assign(currentFile, { thumbnail_data: dataUrl }, updates);
        }
        const targetInList = filesData.find(f => f.id === fileId);
        if (targetInList) {
            Object.assign(targetInList, { thumbnail_data: dataUrl }, updates);
        }
        applySearchAndRender();
    }

    async function runFolderReset({ button, endpoint, confirmText, errorText, updates }) {
        if (!confirm(confirmText)) return;

        const scope = captureScope();
        button.disabled = true;
        try {
            const send = () => vaultFetch(endpoint, { method: 'POST', scope, discardBody:true });
            const res = endpoint === '/api/folders/reset_watch'
                ? await resetPendingWatches(null, scope, send) : await send();
            if (res.ok) {
                await loadMediaFiles();
            } else {
                alert(errorText);
            }
        } catch (err) {
            console.error(errorText, err);
            alert(errorText);
        } finally {
            button.disabled = false;
        }
    }

    const mediaTypeFilter = document.getElementById('media-type-filter');
    mediaTypeFilter.addEventListener('change', () => applySearchAndRender({resetScroll:true}));
    // 2-1. Search & Filter Handlers
    function clearSearchFilters() {
        clearTimeout(searchDebounceTimer);
        for (const [input, button] of [[searchInput, btnClearSearch], [tagSearchInput, btnClearTagSearch]]) {
            if (input) input.value = '';
            if (button) button.style.display = 'none';
        }
    }

    function getFilteredFiles() {
        const query = (searchInput?.value || '').trim().toLowerCase();
        const tagQuery = (tagSearchInput?.value || '').trim().replace(/^#/, '').trim().toLowerCase();
        return filesData.filter(file => {
            if (mediaTypeFilter.value && file.file_type !== mediaTypeFilter.value) return false;
            if (query && !mediaName(file).toLowerCase().includes(query)
                && !file.rel_path.toLowerCase().includes(query)
                && !(file.series_name || '').toLowerCase().includes(query)) return false;
            return !tagQuery || (file.tags || []).some(tag => tag.name.toLowerCase().includes(tagQuery));
        });
    }

    function applySearchAndRender({ resetScroll = false, revealId = null, metadataOnly = false } = {}) {
        const filtered = getFilteredFiles();
        mediaCountEl.textContent = window.vt("{0}개", filtered.length);
        if (metadataOnly && !resetScroll && filtered.length === mediaVirtualList.items.length && filtered.every((file,index)=>file === mediaVirtualList.items[index])) {
            mediaVirtualList.render();
        } else mediaVirtualList.setItems(filtered, { resetScroll, revealId });
    }

    for (const [input, button] of [[searchInput, btnClearSearch], [tagSearchInput, btnClearTagSearch]]) {
        input?.addEventListener('input', () => {
            if (button) button.style.display = input.value.length > 0 ? 'flex' : 'none';
            clearTimeout(searchDebounceTimer);
            searchDebounceTimer = setTimeout(() => applySearchAndRender({ resetScroll: true }), 120);
        });
        button?.addEventListener('click', () => {
            clearTimeout(searchDebounceTimer);
            input.value = '';
            button.style.display = 'none';
            input.focus();
            applySearchAndRender({ resetScroll: true });
        });
    }

    // 3. Fetch and render media library
    let listRequestVersion = 0;
    const watchDisplayWrites = new Set();
    function beginWatchDisplayWrite(fileId, scope) {
        const write={fileId,scope};watchDisplayWrites.add(write);listRequestVersion++;
        return write;
    }
    function finishWatchDisplayWrite(write) {
        watchDisplayWrites.delete(write);
        if(isFresh(write.scope))listRequestVersion++;
    }
    async function loadMediaFiles({ resetScroll = false, selectFirst = true, bounded = false } = {}) {
        if (privacy.covered || dbSecurity.locked) return;
        const version = ++listRequestVersion;
        try {
            const url=`/api/files?sort_by=${currentSortBy}&order=${currentSortOrder}`;
            const result = bounded ? await metadataRequest(url,{},true) : await(await vaultFetch(url)).json();
            if (version !== listRequestVersion) return;
            // A list can observe a committed watch before its POST response arrives.
            // Keep the local baseline until that response applies the update once.
            const writingIds=new Set([...watchDisplayWrites].filter(write=>isFresh(write.scope)).map(write=>write.fileId));
            const baselines=new Map(writingIds.size?filesData.filter(file=>writingIds.has(file.id)).map(file=>[file.id,file]):[]);
            for(const file of result){
                if(!writingIds.has(file.id))continue;
                const previous=baselines.get(file.id) || (currentFile?.id===file.id?currentFile:null);
                if(previous){file.watch_count=previous.watch_count;file.last_watched_at=previous.last_watched_at;}
            }
            filesData = result;
            const selected = filesData.find(file => file.id === currentFile?.id);
            if (selected) Object.assign(currentFile, selected);
            applySearchAndRender({ resetScroll });

            const filtered = getFilteredFiles().filter(file => !file.missing);
            if (selectFirst && noteEditor.canAutoSelect() && filtered.length > 0 && !currentFile && !inArenaMode && !inTrashMode) {
                selectMedia(filtered[0]);
            }
            if(bounded)updateTrashCount();else await updateTrashCount();
            return true;
        } catch (err) {
            console.error('Error loading media files:', err);
            return false;
        }
    }

    function formatTimeAgo(timestamp) {
        if (!timestamp) return window.vt("미시청");
        const now = Math.floor(Date.now() / 1000);
        const diff = now - timestamp;

        if (diff < 60) return window.vt("방금 전");
        if (diff < 3600) return window.vt("{0}분 전", Math.floor(diff / 60));
        if (diff < 86400) return window.vt("{0}시간 전", Math.floor(diff / 3600));
        if (diff < 86400 * 7) return window.vt("{0}일 전", Math.floor(diff / 86400));

        const date = new Date(timestamp * 1000);
        return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
    }

    function decorateMediaCard(card, file, index, items) {
        card.updateTags?.(file);
        card.classList.toggle('active', currentFile?.id === file.id && !inArenaMode && !inTrashMode);
        const grouped = file.series_id != null;
        card.classList.toggle('series-member', grouped);
        card.classList.toggle('series-start', grouped && items[index - 1]?.series_id !== file.series_id);
        card.classList.toggle('series-end', grouped && items[index + 1]?.series_id !== file.series_id);
    }

    function createMediaCard(file) {
        const card = document.createElement('div');
        card.className = 'media-card';
        card.dataset.id = file.id;
        card.onclick = (e) => {
            if (e.target.tagName === 'INPUT' || e.target.classList.contains('tag-delete-btn') || e.target.classList.contains('card-trash-btn') || e.target.classList.contains('tag-badge')) return;
            if (inArenaMode) exitArena();
            if (inTrashMode) exitTrash();
            selectMedia(file);
        };

        // Top section: Thumbnail + Info
        const cardTop = document.createElement('div');
        cardTop.className = 'media-card-top';

        const thumbDiv = document.createElement('div');
        thumbDiv.className = 'thumb-preview';
        if (file.thumbnail_data || (file.file_type === 'IMAGE' && !file.missing)) {
            const img = document.createElement('img');
            img.src = !file.thumbnail_data ? mediaUrl('/api/media/'+encodeMediaPath(file.rel_path)) : file.thumbnail_data.startsWith('/api/') ? mediaUrl(file.thumbnail_data) : file.thumbnail_data;
            img.loading = 'lazy';
            img.decoding = 'async';
            thumbDiv.appendChild(img);
        } else {
            thumbDiv.textContent = file.file_type === 'VIDEO' ? '🎥' : (file.file_type === 'ARCHIVE' ? '📦' : file.file_type === 'PDF' ? '📄' : file.file_type==='NOTE'?'📝':'🖼️');
        }

        const infoDiv = document.createElement('div');
        infoDiv.className = 'media-info';
    
        const titleRow = document.createElement('div');
        titleRow.style.display = 'flex';
        titleRow.style.justifyContent = 'space-between';
        titleRow.style.alignItems = 'center';
        titleRow.style.gap = '4px';

        const title = document.createElement('div');
        title.className = 'media-title';
        title.textContent = mediaName(file);
        title.title = mediaName(file);
        title.style.flex = '1';

        const cardArenaBtn = document.createElement('button');
        cardArenaBtn.className = 'card-arena-btn';
        cardArenaBtn.textContent = '⚔️';
        cardArenaBtn.title = file.series_id ? window.vt("시리즈 대표 파일을 좌측에 고정하고 아레나 시작") : window.vt("이 미디어를 좌측에 고정하고 아레나 대결 시작");
        cardArenaBtn.onclick = (e) => {
            e.stopPropagation();
            startArena(file.id);
        };

        const cardTrashBtn = document.createElement('button');
        cardTrashBtn.className = 'card-trash-btn';
        cardTrashBtn.textContent = '🗑️';
        cardTrashBtn.title = window.vt("휴지통으로 이동");
        cardTrashBtn.onclick = (e) => {
            e.stopPropagation();
            moveToTrash(file.id);
        };

        titleRow.appendChild(title);
        if (file.file_type === 'ARCHIVE' && file.encrypted) {
            const rename = document.createElement('button');
            rename.className = 'card-rename-btn';rename.textContent = '✎';rename.title = window.vt("이름 변경 (복원 파일명 포함)");
            rename.onclick = e => {e.stopPropagation();nameEditor.open(file);};
            titleRow.appendChild(rename);
        }

        if (['ARCHIVE','VIDEO','IMAGE','PDF'].includes(file.file_type)) {
            const seriesButton = document.createElement('button');
            seriesButton.className = `card-series-btn${file.series_id ? ' in-series' : ''}`;
            seriesButton.textContent = '📚';
            seriesButton.title = file.series_id ? window.vt("시리즈 편집") : window.vt("시리즈 지정");
            seriesButton.onclick = e => { e.stopPropagation(); seriesEditor.open(file); };
            titleRow.appendChild(seriesButton);
        }
        cardArenaBtn.disabled = !!file.missing;
        if(file.file_type!=='NOTE')titleRow.appendChild(cardArenaBtn);
        titleRow.appendChild(cardTrashBtn);

        const metaRow = document.createElement('div');
        metaRow.className = 'media-meta-row';

        const scoreTag = document.createElement('span');
        scoreTag.className = 'score-tag';
        scoreTag.textContent = window.vt("🏆 {0}점", file.score ?? 1000);
        scoreTag.title = file.series_id ? window.vt("시리즈 공통 점수") : window.vt("미디어 점수");

        const meta = document.createElement('span');
        meta.className = 'media-meta';
        const encryptedArchive = ['ARCHIVE','VIDEO','IMAGE','NOTE'].includes(file.file_type) && file.encrypted;
        const mediaType = file.file_type==='NOTE' ? window.vt('🔒 메모') : encryptedArchive ? window.vt("🔒 암호화") : archiveUnsupported(file) ? window.vt("ARCHIVE (RAR/7z 미지원)") : file.file_type;
        meta.textContent = `${mediaType} • ${(file.file_size / (1024 * 1024)).toFixed(1)} MB`;
        if(file.file_type==='NOTE')meta.title=window.vt('암호화된 메모');
        else if (encryptedArchive) meta.title = file.file_type === 'VIDEO' ? window.vt("암호화 완료된 영상") : window.vt(file.file_type==='IMAGE'?"암호화 완료된 이미지":"암호화 완료된 이미지 압축파일");

        metaRow.appendChild(scoreTag);
        metaRow.appendChild(meta);
        if (file.missing) {
            const reconnect = document.createElement('button');
            reconnect.className = 'missing-relink-btn';
            reconnect.textContent = window.vt("파일 없음 · 재연결");
            reconnect.title = window.vt("기록은 보존되어 있습니다. 현재 라이브러리 안에서 이동하거나 이름을 바꾼 원본 파일을 선택하세요.");
            reconnect.onclick = async e => {
                e.stopPropagation();
                if (!confirm(window.vt("이동하거나 이름을 바꾼 동일한 원본 파일을 선택해 주세요. 기존 별점·점수·태그·북마크를 선택한 파일에 연결합니다."))) return;
                const scope = captureScope();
                reconnect.disabled = true;
                try {
                    const res = await vaultFetch(`/api/files/${file.id}/relink`, {method:'POST'});
                    if (!res.ok) throw new Error(await res.text());
                    const result = await res.json();
                    if (result.status === 'success' && isFresh(scope)) await loadMediaFiles();
                } catch (error) { if (error.name !== 'AbortError') alert(error.message); }
                finally { reconnect.disabled = false; }
            };
            infoDiv.appendChild(reconnect);
        }

        const watchMeta = document.createElement('div');
        watchMeta.className = 'watch-meta';

        const watchInfoSpan = document.createElement('span');
        const count = file.watch_count || 0;
        const watchTimeStr = file.last_watched_at ? formatDateTime(file.last_watched_at) : window.vt("미시청");

        if (count > 0 && file.last_watched_at) {
            watchInfoSpan.innerHTML = window.vt("👁️ <span class=\"watch-count-badge\">{0}회</span> ({1})", count, watchTimeStr);
        } else {
            watchInfoSpan.innerHTML = window.vt("👁️ 0회 (미시청)");
        }
        watchInfoSpan.title = file.last_watched_at ? window.vt("최근 재생 일시: {0}", watchTimeStr) : window.vt("아직 시청하지 않음");
        watchMeta.appendChild(watchInfoSpan);

        if (count > 0 || file.last_watched_at) {
            const resetBtn = document.createElement('button');
            resetBtn.className = 'btn-reset-watch';
            resetBtn.title = window.vt("시청 횟수 및 이력 초기화");
            resetBtn.innerHTML = window.vt("↺ 초기화");
            resetBtn.onclick = (e) => {
                e.stopPropagation();
                resetWatchCount(file.id);
            };
            watchMeta.appendChild(resetBtn);
        }

        infoDiv.appendChild(titleRow);
        if (file.series_id) {
            const badge = document.createElement('div');
            badge.className = 'series-badge';
            badge.textContent = window.vt("📚 {0} · {1}번째{2}", file.series_name, file.series_position, file.id === file.series_representative_id ? window.vt(" · 대표") : '');
            badge.title = window.vt("시리즈 공통 점수·별점·태그");
            infoDiv.appendChild(badge);
        }
        infoDiv.appendChild(metaRow);
        infoDiv.appendChild(watchMeta);

        cardTop.appendChild(thumbDiv);
        cardTop.appendChild(infoDiv);
        card.appendChild(cardTop);

        // Bottom section: Rating (Left, under thumbnail) + Tags Container (Right)
        const cardBottom = document.createElement('div');
        cardBottom.className = 'media-card-bottom';

        // Star Rating component (140px width matching thumbnail above)
        const ratingContainer = document.createElement('div');
        ratingContainer.className = 'card-rating-container';

        const ratingStars = [];
        const currentRating = file.rating || 0;
        for (let i = 1; i <= 5; i++) {
            const star = document.createElement('span');
            star.className = `star-icon ${i <= currentRating ? 'active' : ''}`;
            star.textContent = '★';
            star.title = window.vt("{0}{1}점 (클릭: 설정/해제)", file.series_id ? window.vt("시리즈 공통 ") : '', i);

            star.onmouseenter = (e) => {
                e.stopPropagation();
                const allStars = ratingContainer.querySelectorAll('.star-icon');
                allStars.forEach((s, idx) => {
                    if (idx < i) s.classList.add('hovered');
                    else s.classList.remove('hovered');
                });
            };

            star.onmouseleave = (e) => {
                e.stopPropagation();
                const allStars = ratingContainer.querySelectorAll('.star-icon');
                allStars.forEach(s => s.classList.remove('hovered'));
            };

            star.onclick = (e) => {
                e.stopPropagation();
                const newRating = (file.rating === i) ? 0 : i;
                updateFileRating(file.id, newRating);
            };

            ratingContainer.appendChild(star); ratingStars.push(star);
        }

        // Tags Container
        const tagContainer = document.createElement('div');
        tagContainer.className = 'tag-container';

        const inputForm = document.createElement('form');
        inputForm.className = 'tag-input-form';
        const tagInput = document.createElement('input');
        tagInput.type = 'text';
        tagInput.className = 'tag-input';
        inputForm.appendChild(tagInput);
        tagContainer.appendChild(inputForm);

        let composingTag = false;
        const pendingTags = new Set();
        const submitTag = async () => {
            if (composingTag) return;
            const entered = tagInput.value;
            const name = entered.trim().replace(/^#+/, '');
            if (!name) return;
            tagInput.value = '';
            if (pendingTags.has(name) || (file.tags || []).some(tag => tag.name === name)) return;
            pendingTags.add(name);
            const scope = captureScope();
            const saved = await addTag(file.id, name, scope);
            pendingTags.delete(name);
            if (!saved && isFresh(scope) && !tagInput.value) tagInput.value = entered;
        };
        tagInput.addEventListener('compositionstart', () => { composingTag = true; });
        tagInput.addEventListener('compositionend', () => { composingTag = false; });
        tagInput.addEventListener('keydown', e => {
            if (composingTag || e.isComposing || e.keyCode === 229 || e.ctrlKey || e.altKey || e.metaKey) return;
            if (e.key !== 'Enter' && e.key !== ' ' && e.code !== 'Space') return;
            e.preventDefault();
            e.stopPropagation();
            if (!e.repeat) return submitTag();
        });
        inputForm.onsubmit = e => {
            e.preventDefault();
            if (!e.isComposing && e.keyCode !== 229) return submitTag();
        };

        // Refresh names and badges while virtualization preserves focus.
        // Replacing the whole card here would interrupt typing/IME composition.
        let tagsSignature = null;
        const tagBadges = [];
        card.updateTags = updated => {
            file = updated;
            ratingStars.forEach((star,index)=>star.classList.toggle('active',index < (file.rating || 0)));
            title.textContent = title.title = mediaName(file);
            const signature = JSON.stringify([file.series_id, file.tags || []]);
            if (signature === tagsSignature) return;
            tagsSignature = signature;
            tagBadges.splice(0).forEach(badge => badge.remove());
            tagInput.placeholder = file.series_id ? window.vt("+ 시리즈 태그") : window.vt("+ 태그 입력");
            tagInput.title = (file.series_id ? window.vt("시리즈 공통 태그") : window.vt("이 파일의 태그")) + window.vt(" · 스페이스 또는 Enter로 추가");
            (file.tags || []).forEach(tag => {
                const tagBadge = document.createElement('span');
                tagBadge.className = 'tag-badge';
                tagBadge.textContent = `#${tag.name}`;
                tagBadge.title = window.vt("'#{0}'(으)로 검색", tag.name);
                tagBadge.style.cursor = 'pointer';
                tagBadge.onclick = (e) => {
                    e.stopPropagation();
                    if (tagSearchInput) {
                        clearTimeout(searchDebounceTimer);
                        tagSearchInput.value = `#${tag.name}`;
                        if (btnClearTagSearch) btnClearTagSearch.style.display = 'flex';
                        applySearchAndRender({ resetScroll: true });
                    }
                };

                const delBtn = document.createElement('span');
                delBtn.className = 'tag-delete-btn';
                delBtn.textContent = '✕';
                delBtn.title = file.series_id ? window.vt("시리즈 전체에서 태그 삭제") : window.vt("태그 삭제");
                delBtn.onclick = (e) => {
                    e.stopPropagation();
                    removeTag(file.id, tag.id);
                };
                tagBadge.appendChild(delBtn);
                tagContainer.insertBefore(tagBadge, inputForm);
                tagBadges.push(tagBadge);
            });

        };
        card.updateTags(file);

        cardBottom.appendChild(ratingContainer);
        cardBottom.appendChild(tagContainer);
        card.appendChild(cardBottom);
        return card;
    }

    const metadataState=document.getElementById('metadata-state'),metadataStatus=document.getElementById('metadata-status'),metadataCheck=document.getElementById('metadata-check');
    let metadataCheckVersion=0;
    metadataCheck.addEventListener('click',async()=>{
        if(metadataCheck.disabled)return;
        const scope=captureScope(),version=++metadataCheckVersion;
        metadataCheck.disabled=true;
        metadataStatus.textContent=window.vt('변경 결과를 확인하는 중…');
        const ok=await loadMediaFiles({selectFirst:false,bounded:true});
        if(!isFresh(scope)||version!==metadataCheckVersion)return;
        metadataCheck.disabled=false;
        metadataStatus.textContent=window.vt(ok?'최신 목록을 불러왔습니다. 별점·태그를 확인하고 필요한 변경을 다시 해 주세요.':'변경 결과를 확인하지 못했습니다. 연결을 확인하고 다시 시도해 주세요.');
    });
    function metadataFailure(scope,error){
        if(!isFresh(scope))return;
        metadataCheckVersion++;metadataCheck.disabled=false;metadataState.hidden=false;
        metadataStatus.textContent=window.vt('별점·태그 변경 결과를 확인하지 못했습니다. 저장되었을 수 있으니 결과 확인을 눌러 확인해 주세요.');
        console.error('Metadata update failed:',error);
    }
    async function metadataRequest(url,options={},json=false){
        const scope=options.scope||captureScope(),controller=new AbortController();
        const headers=new Headers(options.headers||{});
        if(options.method&&options.method!=='GET')headers.set('X-Vault-Write-Deadline',String(Date.now()+15000));
        let timer;
        try{
            return await Promise.race([
                (async()=>{const response=await vaultFetch(url,{...options,headers,scope,signal:controller.signal,discardBody:!json});return json?await response.json():response;})(),
                new Promise((_,reject)=>{timer=setTimeout(()=>{
                    reject(isFresh(scope)?new Error('Metadata request timed out'):STALE_RESPONSE);controller.abort();
                },15000);})
            ]);
        }finally{clearTimeout(timer);}
    }
    let metadataQueue = null, metadataQueueEpoch = null;
    function queueMetadataUpdate(scope, work) {
        if (!isFresh(scope)) return Promise.reject(STALE_RESPONSE);
        if (metadataQueueEpoch !== scope.epoch) { metadataQueue = null; metadataQueueEpoch = scope.epoch; }
        const run = () => { if (!isFresh(scope)) throw STALE_RESPONSE; return work(); };
        const task = metadataQueue ? metadataQueue.then(run,run) : Promise.resolve(run());
        metadataQueue = task;
        const clear = () => { if (metadataQueue === task) metadataQueue = null; };
        task.then(clear,clear);
        return task;
    }
    async function patchMetadata(fileId, seriesId, patch, scope, sortSensitive = false) {
        if (!isFresh(scope)) throw STALE_RESPONSE;
        // Older list responses must not overwrite this committed update.
        listRequestVersion++;
        const source = filesData.find(file=>file.id===fileId);
        if (!source || source.series_id !== seriesId || sortSensitive) { if(!await loadMediaFiles({bounded:true}))throw new Error('Metadata refresh failed'); return; }
        const matches = file => file && (file.id===fileId || (seriesId != null && file.series_id===seriesId));
        for (const file of filesData) if (matches(file)) patch(file);
        if (matches(currentFile) && !filesData.includes(currentFile)) patch(currentFile);
        applySearchAndRender({metadataOnly:true});
    }
    async function updateFileRating(fileId, newRating) {
        const scope = captureScope();
        try {
            await queueMetadataUpdate(scope, async()=>{
                const seriesId = filesData.find(file=>file.id===fileId)?.series_id;
                await metadataRequest(`/api/files/${fileId}/rating`, {scope,method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({rating:newRating})});
                await patchMetadata(fileId,seriesId,file=>{file.rating=Math.max(0,Math.min(5,newRating));},scope,currentSortBy==='rating');
            });
        } catch (err) { if (err!==STALE_RESPONSE) metadataFailure(scope,err); }
    }
    async function addTag(fileId, tagName, scope = captureScope()) {
        try {
            await queueMetadataUpdate(scope,async()=>{
                const seriesId = filesData.find(file=>file.id===fileId)?.series_id;
                const tag = await metadataRequest(`/api/files/${fileId}/tags`, {scope,method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({name:tagName})},true);
                await patchMetadata(fileId,seriesId,file=>{file.tags=[...(file.tags||[]).filter(existing=>existing.id!==tag.id),tag].sort((a,b)=>a.name.localeCompare(b.name));},scope);
            });
            return true;
        } catch (err) { if (err!==STALE_RESPONSE) metadataFailure(scope,err); return false; }
    }
    async function removeTag(fileId, tagId) {
        const scope = captureScope();
        try {
            await queueMetadataUpdate(scope,async()=>{
                const seriesId = filesData.find(file=>file.id===fileId)?.series_id;
                await metadataRequest(`/api/files/${fileId}/tags/${tagId}`,{scope,method:'DELETE'});
                await patchMetadata(fileId,seriesId,file=>{file.tags=(file.tags||[]).filter(tag=>tag.id!==tagId);},scope);
            });
        } catch (err) { if (err!==STALE_RESPONSE) metadataFailure(scope,err); }
    }

    function isWatching(session) {
        return session && watchSession === session && isFresh(session.scope) &&
            session.selection === selectionVersion && currentFile?.id === session.id && !inArenaMode && !inTrashMode;
    }

    function qualifyWatch(session) {
        if (!isWatching(session) || session.counted) return;
        if (session.seconds < 10 && session.pages.size < 5) return;
        // Claim this viewing session once; a rejected write stays queued even if
        // the viewer moves to another file before a background check finishes.
        session.counted = true;
        const record = {fileId:session.id, scope:session.scope, sending:false};
        pendingWatchRecords.add(record);
        sendWatchRecord(record);
    }

    function activeVideoWatch() {
        return isWatching(watchSession) && watchSession.type === 'VIDEO' && video.src === watchSession.videoSrc
            ? watchSession : null;
    }

    function sampleVideoWatch() {
        const session = activeVideoWatch();
        if (!session || session.counted || !session.sample) return;
        const now = performance.now();
        const elapsed = Math.max(0, (now - session.sample.at) / 1000);
        const advanced = video.currentTime - session.sample.position;
        // Count real playback time. Seeking, buffering and faster playback cannot
        // turn skipped media time into watched seconds.
        if (!video.seeking && advanced > 0) {
            session.seconds += Math.min(elapsed, advanced / session.sample.rate);
        }
        session.sample = {at:now, position:video.currentTime, rate:video.playbackRate || 1};
        qualifyWatch(session);
    }

    function startVideoWatch() {
        const session = activeVideoWatch();
        if (!session || session.counted || video.paused || video.seeking) return;
        session.sample = {at:performance.now(), position:video.currentTime, rate:video.playbackRate || 1};
    }

    function stopVideoWatch(countTail = true) {
        if (countTail) sampleVideoWatch();
        if (watchSession) watchSession.sample = null;
    }

    function trackVisibleArchivePage(image, index, token) {
        const session = watchSession;
        if (!image || !isWatching(session) || session.type !== 'ARCHIVE' || session.counted) return;
        image.decode().then(() => {
            if (!isWatching(session) || token !== archiveLoadToken || image.naturalWidth <= 0) return;
            session.pages.add(index);
            qualifyWatch(session);
        }).catch(() => {}); // Failed or superseded images were not viewed.
    }

    function watchResetMatches(reset, record) {
        return reset.scope.generation === record.scope.generation &&
            (reset.fileId === null || reset.fileId === record.fileId);
    }

    function watchIsResetting(record) {
        return [...watchResets].some(reset => watchResetMatches(reset, record));
    }

    async function resetPendingWatches(fileId, scope, send) {
        const reset = {fileId, scope};
        watchResets.add(reset);
        try {
            // Finish already submitted writes before resetting, and hold queued
            // records so an earlier viewing cannot reappear after a reset.
            await Promise.allSettled([...pendingWatchRecords]
                .filter(record => record.sending && watchResetMatches(reset, record))
                .map(record => record.promise));
            if (!isFresh(scope)) throw STALE_RESPONSE;
            const response = await send();
            if (response.ok) {
                for (const record of pendingWatchRecords) {
                    if (watchResetMatches(reset, record)) pendingWatchRecords.delete(record);
                }
            }
            return response;
        } finally {
            watchResets.delete(reset);
            scheduleWatchRetry();
        }
    }

    function sendWatchRecord(record) { record.promise = recordWatch(record); }

    function scheduleWatchRetry() {
        if (![...pendingWatchRecords].some(record => !record.sending && !watchIsResetting(record))) {
            clearTimeout(watchRetryTimer); watchRetryTimer = null; return;
        }
        if (watchRetryTimer !== null) return;
        watchRetryTimer = setTimeout(() => {
            watchRetryTimer = null;
            for (const record of pendingWatchRecords) {
                if (!record.sending && !watchIsResetting(record)) sendWatchRecord(record);
            }
        }, 2000);
    }

    async function recordWatch(record) {
        const {fileId, scope} = record;
        if (record.sending || !pendingWatchRecords.has(record) || watchIsResetting(record)) return;
        if (!isFresh(scope)) { pendingWatchRecords.delete(record); return; }
        record.sending = true;
        const displayWrite=beginWatchDisplayWrite(fileId,scope);
        let retry = false;
        try {
            await vaultFetch(`/api/files/${fileId}/watch`, {
                discardBody:true,
                scope, method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({})
            });
            listRequestVersion++;
            // Do not retry a committed write if refreshing the view later fails.
            pendingWatchRecords.delete(record);
            const now = Math.floor(Date.now() / 1000);
            const file = filesData.find(f => f.id === fileId);
            if (file) {
                file.last_watched_at = now;
                file.watch_count = (file.watch_count || 0) + 1;
            }
            if (currentFile && currentFile.id === fileId && currentFile !== file) {
                currentFile.last_watched_at = now;
                currentFile.watch_count = (currentFile.watch_count || 0) + 1;
            }
            if (currentSortBy === 'watch_count' || currentSortBy === 'last_watched') await loadMediaFiles();
            else applySearchAndRender();
        } catch (err) {
            // A 409 explicitly rejects the write while inspection or another
            // exclusive job is active. Unknown network outcomes cannot be
            // safely retried because they may already have incremented the DB.
            retry = err.status === 409 && isFresh(scope) && pendingWatchRecords.has(record);
            if (!retry && err !== STALE_RESPONSE) console.error('Failed to record watch history:', err);
        } finally {
            finishWatchDisplayWrite(displayWrite);
            record.sending = false;
            if (!retry) pendingWatchRecords.delete(record);
            scheduleWatchRetry();
        }
    }

    async function resetWatchCount(fileId) {
        const scope = captureScope();
        const displayWrite=beginWatchDisplayWrite(fileId,scope);
        try {
            const res = await resetPendingWatches(fileId, scope, () =>
                vaultFetch(`/api/files/${fileId}/reset_watch`, {method:'POST', scope,discardBody:true}));
            if (res.ok) {
                const file = filesData.find(f => f.id === fileId);
                if (file) {
                    file.watch_count = 0;
                    file.last_watched_at = null;
                }
                if (currentFile && currentFile.id === fileId) {
                    currentFile.watch_count = 0;
                    currentFile.last_watched_at = null;
                }
                if (currentSortBy === 'watch_count' || currentSortBy === 'last_watched') await loadMediaFiles();
            else applySearchAndRender();
            }
        } catch (err) {
            console.error('Failed to reset watch count:', err);
        } finally { finishWatchDisplayWrite(displayWrite); }
    }

    async function selectMedia(file, { archivePage = 'first' } = {}) {
        if (!file || file.missing) return;
        if(file.file_type==='NOTE'&&!noteEditor.canOpen(file))return;
        if(!noteEditor.leave())return;
        document.getElementById('note-stage').hidden=true;
        pdfViewer.reset();
        viewerImage.removeAttribute('src'); viewerImage2?.removeAttribute('src');
        const scope = captureScope();
        const selection = ++selectionVersion;
        watchSession = {id:file.id, type:file.file_type, scope, selection, seconds:0, pages:new Set(), counted:false, sample:null};
        archiveLoadToken += 1;
        if (videoMetadataCleanup) { videoMetadataCleanup(); videoMetadataCleanup = null; }
        resetImageCaptureButton();
        clearImageJobs();
        currentFile = file;
        resetBookmarks();
        setArchiveEntries([]);
        if (file.file_type !== 'ARCHIVE'&&file.file_type!=='NOTE') loadBookmarks();
        mediaVirtualList.reveal(file.id);

        const encodedPath = encodeMediaPath(file.rel_path);

        if (file.file_type === 'VIDEO') {
            videoStage.style.display = 'flex';
            imageStage.style.display = 'none';
            arenaStage.style.display = 'none';
            trashStage.style.display = 'none';
            video.src = mediaUrl(`/api/media/${encodedPath}`);
            watchSession.videoSrc = video.src;
            video.load();
            video.playbackRate = parseFloat(speedSelect.value);
            const expectedSrc = video.src;
            const onMetadata = () => {
                videoMetadataCleanup = null;
                if (!isFresh(scope) || selection !== selectionVersion || currentFile?.id !== file.id || video.src !== expectedSrc) return;
                durationEl.textContent = formatTime(video.duration);
                if (video.duration) {
                    vaultFetch(`/api/duration/${file.id}`, {
                        discardBody:true,
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ duration_secs: Math.round(video.duration) }),
                        scope
                    }).catch(err => { if (err !== STALE_RESPONSE) console.error(err); });
                }
            };
            video.addEventListener('loadedmetadata', onMetadata, { once: true });
            videoMetadataCleanup = () => video.removeEventListener('loadedmetadata', onMetadata);
            video.play().catch(err => console.log('Autoplay deferred:', err));
        } else if (file.file_type === 'ARCHIVE') {
            videoStage.style.display = 'none';
            imageStage.style.display = 'flex';
            arenaStage.style.display = 'none';
            trashStage.style.display = 'none';
            video.pause();
            const archiveExt = file.rel_path.split('.').pop().toLowerCase();
            if (archiveExt === 'rar' || archiveExt === '7z') {
                setArchiveEntries([]);
                setArchiveDisplayMode(false);
                viewerImage.removeAttribute('src');
                imgEntryName.textContent = window.vt("{0} (지원하지 않는 압축 형식)", file.rel_path);
                imgPageCounter.textContent = window.vt("RAR/7z는 현재 지원되지 않습니다.");
                return;
            }
            try {
                const res = await vaultFetch(`/api/archive/list/${encodedPath}`);
                const entries = await res.json();
                if (!isFresh(scope) || selection !== selectionVersion) return;
                setArchiveEntries(entries);
                loadBookmarks();
                currentImageIdx = archivePage === 'last' ? Math.max(0, archiveEntries.length - 1) : Number.isInteger(archivePage) ? Math.max(0, Math.min(archivePage,archiveEntries.length-1)) : 0;
                currentImageStep = 1;
                imageMetaCache.clear();
                if (archiveEntries.length > 0) {
                    showArchiveImage(encodedPath, currentImageIdx);
                }
            } catch (err) {
                if (!isFresh(scope) || selection !== selectionVersion) return;
                viewerImage.removeAttribute('src');
                setArchiveDisplayMode(false);
                console.error('Failed to load archive entries:', err);
                imgEntryName.textContent = window.vt("압축파일을 열 수 없습니다.");
                imgPageCounter.textContent = err.message;
            }
        } else if (file.file_type === 'NOTE') {
            video.pause();video.removeAttribute('src');video.load();
            videoStage.style.display='none';imageStage.style.display='none';arenaStage.style.display='none';trashStage.style.display='none';
            document.getElementById('note-stage').hidden=false;
            document.getElementById('note-stage-title').textContent=file.display_name||window.vt('제목 없는 메모');
            await noteEditor.open(file);
        } else if (file.file_type === 'PDF') {
            video.pause(); video.removeAttribute('src'); video.load();
            videoStage.style.display = 'none'; imageStage.style.display = 'none';
            arenaStage.style.display = 'none'; trashStage.style.display = 'none';
            await pdfViewer.open(file, mediaUrl(`/api/media/${encodedPath}`));
        } else if (file.file_type === 'IMAGE') {
            videoStage.style.display = 'none';
            imageStage.style.display = 'flex';
            arenaStage.style.display = 'none';
            trashStage.style.display = 'none';
            video.pause();
            setArchiveEntries([file.rel_path]);
            currentImageIdx = 0;
            currentImageStep = 1;
            viewerImage.src = mediaUrl(`/api/media/${encodedPath}`);
            setArchiveDisplayMode(false);
            syncArchiveSeekBar(0);
            imgEntryName.textContent = file.rel_path;
            imgPageCounter.textContent = `1 / 1 (100%)`;
        }
    }

    function navigateImage(direction) {
        const images = getFilteredFiles().filter(file => !file.missing && file.file_type === 'IMAGE');
        const index = images.findIndex(file => file.id === currentFile?.id);
        const target = images[index + direction];
        if (index >= 0 && target) return selectMedia(target);
    }

    function navigateMedia(direction) {
        const filtered = getFilteredFiles().filter(file => !file.missing);
        if (!filtered || filtered.length === 0) return;

        let currentIndex = -1;
        if (currentFile) {
            currentIndex = filtered.findIndex(f => f.id === currentFile.id);
        }

        if (currentIndex === -1) {
            selectMedia(filtered[0]);
            return;
        }

        let targetIndex = currentIndex;
        if (direction === 'prev') {
            if (currentIndex > 0) {
                targetIndex = currentIndex - 1;
            }
        } else if (direction === 'next') {
            if (currentIndex < filtered.length - 1) {
                targetIndex = currentIndex + 1;
            }
        }

        if (targetIndex !== currentIndex && filtered[targetIndex]) {
            selectMedia(filtered[targetIndex]);
        }
    }

    // 이미지 메타데이터 비동기 병렬 캐싱 함수
    function metaKey(encodedPath, entryName) { return `${vaultSession.generation}:${requestEpoch}:${encodedPath}\n${entryName}`; }
    function drainImageMetaQueue() {
        while (imageMetaActive < IMAGE_META_CONCURRENCY && imageMetaQueue.length) {
            // Keep one slot available for a newly selected/visible page.
            if (imageMetaQueue[0].priority > 0 && imagePrefetchActive >= 1) break;
            const job = imageMetaQueue.shift();
            const speculative = job.priority > 0, started = performance.now();
            imageMetaActive += 1;
            if (speculative) imagePrefetchActive++;
            let finished = false;
            let img = null;
            const cancel = () => { if (img) { img.onload = img.onerror = null; img.src = ""; } finish(null); };
            const finish = (meta) => {
                if (finished) return;
                finished = true;
                metaCancels.delete(cancel);
                imageMetaActive -= 1;
                if (speculative) imagePrefetchActive--;
                imageMetaPending.delete(job.key);
                if (meta && isFresh(job.scope) && job.selection === selectionVersion) {
                    const elapsed = Math.max(1,performance.now()-started);
                    imageLoadMs = imageLoadMs === null ? elapsed : imageLoadMs * .75 + elapsed * .25;
                    imageLoadPixels = meta.width * meta.height;
                    imageMetaCache.set(job.key, meta);
                    while (imageMetaCache.size > IMAGE_META_CACHE_MAX) imageMetaCache.delete(imageMetaCache.keys().next().value);
                }
                job.resolve(meta || { isPortrait: false, width: 0, height: 0 });
                drainImageMetaQueue();
            };
            if (!isFresh(job.scope) || job.selection !== selectionVersion || !currentFile || encodeMediaPath(currentFile.rel_path) !== job.encodedPath) {
                finish(null);
                continue;
            }
            img = new Image();
            metaCancels.add(cancel);
            img.onload = () => finish({ width: img.naturalWidth, height: img.naturalHeight, isPortrait: img.naturalHeight > img.naturalWidth });
            img.onerror = () => finish(null);
            img.src = job.url;
        }
    }

    function getImageMeta(encodedPath, entryName, priority = 0) {
        if (!entryName) return Promise.resolve({ isPortrait: false, width: 0, height: 0 });
        const key = metaKey(encodedPath, entryName);
        if (imageMetaCache.has(key)) {
            const meta = imageMetaCache.get(key);
            imageMetaCache.delete(key); imageMetaCache.set(key, meta);
            return Promise.resolve(meta);
        }
        if (imageMetaPending.has(key)) {
            const pending = imageMetaPending.get(key);
            const queued = imageMetaQueue.find(job=>job.key===key);
            if (queued && priority < queued.priority) {
                queued.priority = priority; imageMetaQueue.sort((a,b)=>a.priority-b.priority); drainImageMetaQueue();
            }
            return pending;
        }
        let promiseResolve;
        const promise = new Promise((resolve) => { promiseResolve = resolve; });
        imageMetaPending.set(key, promise);
        imageMetaQueue.push({ key, encodedPath, entryName, priority, scope: captureScope(), selection: selectionVersion,
            url: mediaUrl(`/api/archive/file/${encodedPath}`, { entry: entryName }), resolve: promiseResolve });
        imageMetaQueue.sort((a,b)=>a.priority-b.priority);
        while (imageMetaQueue.length > 32) {
            const obsolete = imageMetaQueue.pop(); imageMetaPending.delete(obsolete.key);
            obsolete.resolve({ isPortrait: false, width: 0, height: 0 });
        }
        drainImageMetaQueue();
        return promise;
    }

    // Always cover the next two images; vary only the more distant lookahead.
    function preloadArchiveImages(encodedPath, startIdx) {
        const count = imageLoadMs !== null && (imageLoadMs > 500 || imageLoadPixels > 16000000) ? 2 : imageLoadMs !== null && imageLoadMs < 150 ? 6 : 4;
        const end = Math.min(archiveEntries.length,startIdx+count);
        const wanted = new Set(archiveEntries.slice(startIdx,end).map(entry=>metaKey(encodedPath,entry)));
        // Drop obsolete queued speculation, never cancel an in-flight read on a page turn.
        for (let i=imageMetaQueue.length-1;i>=0;i--) {
            const job=imageMetaQueue[i];
            if (job.priority > 0 && !wanted.has(job.key)) {
                imageMetaQueue.splice(i,1);imageMetaPending.delete(job.key);
                job.resolve({isPortrait:false,width:0,height:0});
            }
        }
        for (let i=startIdx;i<end;i++) {
            const entry=archiveEntries[i];
            if (entry && !imageMetaCache.has(metaKey(encodedPath,entry))) getImageMeta(encodedPath,entry,i<startIdx+2?1:2);
        }
    }

    async function showArchiveImage(encodedPath, idx, navigation = 'jump') {
        if (idx < 0 || idx >= archiveEntries.length) return;
        bookmarkPreparationCancel?.(new Error(window.vt("페이지가 바뀌었습니다. 다시 저장해 주세요.")));
        if (navigation === 'next') archiveBackStack.push(currentImageIdx);
        else if (navigation !== 'previous') archiveBackStack = [];
        currentImageIdx = idx;
        const thisToken = ++archiveLoadToken;

        const entry1 = archiveEntries[idx];
        const entry2 = (idx + 1 < archiveEntries.length) ? archiveEntries[idx + 1] : null;

        const url1 = mediaUrl(`/api/archive/file/${encodedPath}`, { entry: entry1 });
        const url2 = entry2 ? mediaUrl(`/api/archive/file/${encodedPath}`, { entry: entry2 }) : null;

        const cached1 = imageMetaCache.get(metaKey(encodedPath, entry1));
        const cached2 = entry2 ? imageMetaCache.get(metaKey(encodedPath, entry2)) : null;

        // [초고속 모드 1] 캐시에 두 이미지가 모두 세로형으로 확인된 경우: 0ms 즉시 2페이지 렌더링
        if (cached1 && cached2 && cached1.isPortrait && cached2.isPortrait) {
            viewerImage.src = url1;
            trackVisibleArchivePage(viewerImage, idx, thisToken);
            setArchiveDisplayMode(true);
            if (viewerImage2) {
                viewerImage2.src = url2;
                trackVisibleArchivePage(viewerImage2, idx + 1, thisToken);
            }
            syncArchiveSeekBar(idx);
            const pct2 = Math.round(((idx + 2) / archiveEntries.length) * 100);
            imgEntryName.textContent = `${entry1} & ${entry2}`;
            imgPageCounter.textContent = `${idx + 1}-${idx + 2} / ${archiveEntries.length} (${pct2}%)`;
            currentImageStep = 2;
            preloadArchiveImages(encodedPath, idx + 2);
            return;
        }

        // [초고속 모드 2] 첫 번째 이미지가 가로형으로 확인된 경우: 0ms 즉시 1페이지 렌더링
        if (cached1 && !cached1.isPortrait) {
            viewerImage.src = url1;
            trackVisibleArchivePage(viewerImage, idx, thisToken);
            setArchiveDisplayMode(false);
            syncArchiveSeekBar(idx);
            const pct1 = Math.round(((idx + 1) / archiveEntries.length) * 100);
            imgEntryName.textContent = entry1;
            imgPageCounter.textContent = `${idx + 1} / ${archiveEntries.length} (${pct1}%)`;
            currentImageStep = 1;
            preloadArchiveImages(encodedPath, idx + 1);
            return;
        }

        // [최초 로딩 모드] 병렬로 동시 로드 및 판별
        viewerImage.src = url1;
        trackVisibleArchivePage(viewerImage, idx, thisToken);
        setArchiveDisplayMode(false);
        syncArchiveSeekBar(idx);
        const pct1 = Math.round(((idx + 1) / archiveEntries.length) * 100);
        imgEntryName.textContent = entry1;
        imgPageCounter.textContent = `${idx + 1} / ${archiveEntries.length} (${pct1}%)`;
        currentImageStep = 1;

        // 1번과 2번 이미지를 직렬이 아닌 "병렬(Parallel)"로 동시 요청
        const promises = [getImageMeta(encodedPath, entry1)];
        if (entry2) promises.push(getImageMeta(encodedPath, entry2));

        const [meta1, meta2] = await Promise.all(promises);
        if (thisToken !== archiveLoadToken) return;

        if (meta1.isPortrait && meta2 && meta2.isPortrait && viewerImage2 && imageDisplayBox) {
            viewerImage2.src = url2;
            trackVisibleArchivePage(viewerImage2, idx + 1, thisToken);
            setArchiveDisplayMode(true);
            const pct2 = Math.round(((idx + 2) / archiveEntries.length) * 100);
            imgEntryName.textContent = `${entry1} & ${entry2}`;
            imgPageCounter.textContent = `${idx + 1}-${idx + 2} / ${archiveEntries.length} (${pct2}%)`;
            currentImageStep = 2;
        }

        preloadArchiveImages(encodedPath, idx + currentImageStep);
    }

    function nextArchiveImage() {
        if (currentFile?.file_type === 'IMAGE') return navigateImage(1);
        if (!currentFile || currentFile.file_type !== 'ARCHIVE' || archiveEntries.length === 0) return;
        const encodedPath = encodeMediaPath(currentFile.rel_path);
        const step = currentImageStep || 1;
        const nextIdx = currentImageIdx + step;
        if (nextIdx >= archiveEntries.length) {
            const files = getFilteredFiles();
            const index = files.findIndex(file => file.id === currentFile.id);
            if (index < 0) return;
            const next = files.slice(index + 1).find(file =>
                !file.missing && file.file_type === 'ARCHIVE' && /\.(zip|tar\.zst)$/i.test(file.rel_path));
            if (next) return selectMedia(next);
            return;
        }
        return showArchiveImage(encodedPath, nextIdx, 'next');
    }

    async function prevArchiveImage() {
        if (currentFile?.file_type === 'IMAGE') return navigateImage(-1);
        if (!currentFile || currentFile.file_type !== 'ARCHIVE' || archiveEntries.length === 0) return;
        const encodedPath = encodeMediaPath(currentFile.rel_path);
        if (currentImageIdx === 0) {
            const files = getFilteredFiles();
            const index = files.findIndex(file => file.id === currentFile.id);
            if (index < 0) return;
            const previous = files.slice(0, index).reverse().find(file =>
                !file.missing && file.file_type === 'ARCHIVE' && /\.(zip|tar\.zst)$/i.test(file.rel_path));
            if (previous) return selectMedia(previous, { archivePage: 'last' });
            return;
        }
        let prevIdx = archiveBackStack.pop();
        if (prevIdx == null) {
            // A seek/bookmark has no navigation history. Inspect the preceding
            // pages, not the width of the spread currently on screen.
            prevIdx = currentImageIdx - 1;
            if (currentImageIdx >= 2) {
                const token = archiveLoadToken;
                const [left, right] = await Promise.all([
                    getImageMeta(encodedPath, archiveEntries[currentImageIdx - 2]),
                    getImageMeta(encodedPath, archiveEntries[currentImageIdx - 1])
                ]);
                if (token !== archiveLoadToken) return;
                if (left.isPortrait && right.isPortrait) prevIdx--;
            }
        }
        return showArchiveImage(encodedPath, prevIdx, 'previous');
    }

    btnImgPrev.addEventListener('click', prevArchiveImage);
    btnImgNext.addEventListener('click', nextArchiveImage);

    // Image Seek Bar Drag & Hover System
    if (imageSeekBar) {
        imageSeekBar.addEventListener('input', () => {
            if (currentFile && currentFile.file_type === 'ARCHIVE' && archiveEntries.length > 0) {
                const targetIdx = Math.max(0, Math.min(archiveEntries.length - 1, parseInt(imageSeekBar.value) - 1));
                const encodedPath = encodeMediaPath(currentFile.rel_path);
                showArchiveImage(encodedPath, targetIdx);
            }
        });
    }

    // 이미지 화면 클릭 시 마우스의 화면 X축 위치와 일치하는 페이지로 즉시 이동
    if (imageWrapper) {
        imageWrapper.addEventListener('click', (e) => {
            if (e.target.closest('.img-nav-btn')) return;
            if (!currentFile || currentFile.file_type !== 'ARCHIVE' || archiveEntries.length === 0) return;
            if (!imageSeekWrapper) return;
            const { ratio } = getPointerProgress(imageSeekWrapper, e.clientX);
            const targetIdx = Math.max(0, Math.min(archiveEntries.length - 1, Math.round(ratio * (archiveEntries.length - 1))));
            const encodedPath = encodeMediaPath(currentFile.rel_path);
            showArchiveImage(encodedPath, targetIdx);
        });

        // 이미지 화면 마우스 이동 시 진행바에 마우스 커서와 수직으로 일치하는 위치 및 페이지 표시
        imageWrapper.addEventListener('mousemove', (e) => {
            if (e.target.closest('.img-nav-btn')) {
                hideHoverMarker(imageSeekHoverIndicator, imageSeekHoverTooltip);
                return;
            }
            if (!currentFile || currentFile.file_type !== 'ARCHIVE' || archiveEntries.length === 0) return;
            if (!imageSeekWrapper) return;
            const { x, ratio } = getPointerProgress(imageSeekWrapper, e.clientX);
            const targetPage = Math.max(1, Math.min(archiveEntries.length, Math.round(ratio * (archiveEntries.length - 1)) + 1));
            const pct = Math.round((targetPage / archiveEntries.length) * 100);
            showHoverMarker(imageSeekHoverIndicator, imageSeekHoverTooltip, x, `${targetPage} / ${archiveEntries.length} (${pct}%)`);
        });

        imageWrapper.addEventListener('mouseleave', () => {
            hideHoverMarker(imageSeekHoverIndicator, imageSeekHoverTooltip);
        });
    }

    if (imageSeekWrapper && imageSeekHoverIndicator && imageSeekHoverTooltip) {
        imageSeekWrapper.addEventListener('mousemove', (e) => {
            if (!currentFile || currentFile.file_type !== 'ARCHIVE' || archiveEntries.length === 0) return;
            const { x, ratio } = getPointerProgress(imageSeekWrapper, e.clientX);
            const targetPage = Math.max(1, Math.min(archiveEntries.length, Math.round(ratio * (archiveEntries.length - 1)) + 1));
            const pct = Math.round((targetPage / archiveEntries.length) * 100);
            showHoverMarker(imageSeekHoverIndicator, imageSeekHoverTooltip, x, `${targetPage} / ${archiveEntries.length} (${pct}%)`);
        });

        imageSeekWrapper.addEventListener('mouseleave', () => {
            hideHoverMarker(imageSeekHoverIndicator, imageSeekHoverTooltip);
        });
    }

    // ==========================================
    // Trash Bin & Deletion Audit Log System
    // ==========================================
    async function updateTrashCount() {
        if(privacy.covered)return;
        if (dbSecurity.locked) return;
        const scope=captureScope(),version=++trashCountVersion;
        try {
            const res = await vaultFetch('/api/trash/count',{scope});
            const result = await res.json();
            if(!isFresh(scope)||version!==trashCountVersion)return;
            const count = Number(result.count || 0);
            trashCountEl.textContent = count;
            sidebarTrashCountEl.textContent = count;
        } catch (err) {
            console.error('Failed to update trash count:', err);
        }
    }

    function clearCurrentMedia() {
        // Clear the removed media even when leave() retains an open note draft.
        noteEditor.leave();
        pdfViewer.reset();
        document.getElementById('note-stage').hidden=true;
        document.getElementById('note-stage-title').textContent='';
        currentFile=null; watchSession=null; selectionVersion++; archiveLoadToken++;
        clearImageJobs(); resetBookmarks(); setArchiveEntries([]);
        if(videoMetadataCleanup){videoMetadataCleanup();videoMetadataCleanup=null;}
        resetImageCaptureButton();
        video.pause();video.removeAttribute('src');video.load();
        viewerImage.removeAttribute('src');viewerImage2?.removeAttribute('src');
        imageStage.style.display='none';videoStage.style.display='none';
    }

    let trashMutation=null;
    function updateTrashItemControls(){
        const busy=!!trashMutation;
        btnEmptyTrash.disabled=busy;
        trashItemsGrid.querySelectorAll('button').forEach(button=>{button.disabled=busy;});
    }
    function beginTrashMutation(){
        if(trashMutation)return null;
        const attempt={scope:captureScope()};trashMutation=attempt;
        trashItemsLoadVersion++;trashCountVersion++;
        document.getElementById('trash-items-status').textContent='';
        updateTrashItemControls();return attempt;
    }
    async function finishTrashMutation(attempt){
        if(trashMutation!==attempt)return;
        if(isFresh(attempt.scope)){
            // Invalidate reads sent before the mutation completed, including
            // reads started while the operation was still waiting on the DB.
            trashItemsLoadVersion++;trashCountVersion++;
            await Promise.allSettled([loadTrashItems(),loadTrashLogs(),loadMediaFiles(),updateTrashCount()]);
        }
        if(trashMutation===attempt){trashMutation=null;updateTrashItemControls();}
    }
    async function mutateTrash(fileId,action){
        const attempt=beginTrashMutation();if(!attempt)return;
        try{
            await vaultFetch('/api/files/'+fileId+'/'+action,{method:action==='permanent'?'DELETE':'POST',scope:attempt.scope,discardBody:true});
            if(action==='trash'&&currentFile?.id===fileId)clearCurrentMedia();
        }catch(error){
            if(isFresh(attempt.scope))document.getElementById('trash-items-status').textContent=error.message;
        }finally{await finishTrashMutation(attempt);}
    }
    async function moveToTrash(fileId){await mutateTrash(fileId,'trash');}
    async function restoreFromTrash(fileId){await mutateTrash(fileId,'restore');}
    async function permanentlyDelete(fileId){
        if(trashMutation)return;
        if(!confirm(window.vt("정말로 이 파일을 디스크에서 영구 삭제하시겠습니까?\n(삭제된 파일 정보는 삭제 이력에 기록됩니다)")))return;
        await mutateTrash(fileId,'permanent');
    }
    async function emptyTrash() {
        const attempt=beginTrashMutation();if(!attempt)return;
        const scope=attempt.scope,status=document.getElementById('trash-items-status');
        let deleted=0,total=0;
        try {
            const trashItems=await(await vaultFetch('/api/trash',{scope})).json();
            total=trashItems.length;
            if(!total){alert(window.vt("휴지통이 이미 비어 있습니다."));return;}
            if(!confirm(window.vt("휴지통의 모든 파일({0}개)을 디스크에서 영구 삭제하시겠습니까?\n(삭제된 모든 파일의 기록은 삭제 이력에 보존됩니다)",total)))return;
            for(const item of trashItems){
                await vaultFetch('/api/files/'+item.id+'/permanent',{method:'DELETE',scope,discardBody:true});
                deleted++;
                status.textContent=window.vt('영구 삭제 완료 {0}/{1}개',deleted,total);
            }
        } catch(error) {
            if(isFresh(scope))status.textContent=window.vt('영구 삭제 중단 · 완료 {0}/{1}개. 나머지 항목은 휴지통에서 확인해 주세요.',deleted,total)+'\n'+error.message;
        } finally { await finishTrashMutation(attempt); }
    }

    async function loadTrashItems() {
        const scope=captureScope(),version=++trashItemsLoadVersion,countVersion=++trashCountVersion;
        try {
            const res = await vaultFetch('/api/trash',{scope});
            const items = await res.json();
            if(!isFresh(scope)||version!==trashItemsLoadVersion)return;
            if(countVersion===trashCountVersion){
                trashCountEl.textContent = items.length;
                sidebarTrashCountEl.textContent = items.length;
            }

            trashItemsGrid.innerHTML = '';
            if (items.length === 0) {
                trashItemsGrid.innerHTML = window.vt("<div style=\"color: var(--text-muted); font-size: 12px; padding: 20px; text-align: center; grid-column: 1 / -1;\">휴지통이 비어 있습니다.</div>");
                return;
            }

            items.forEach(item => {
                const card = document.createElement('div');
                card.className = 'trash-card';

                const thumb = document.createElement('div');
                thumb.className = 'trash-thumb-preview';
                if (item.thumbnail_data) {
                    const img = document.createElement('img');
                    img.src = item.thumbnail_data.startsWith('/api/') ? mediaUrl(item.thumbnail_data) : item.thumbnail_data;
                    thumb.appendChild(img);
                } else {
                    thumb.textContent = item.file_type === 'VIDEO' ? '🎥' : (item.file_type === 'ARCHIVE' ? '📦' : item.file_type === 'PDF' ? '📄' : '🖼️');
                }

                const info = document.createElement('div');
                info.className = 'trash-card-info';

                const title = document.createElement('div');
                title.className = 'trash-card-title';
                title.textContent = mediaName(item);
                title.title = mediaName(item);

                const meta = document.createElement('div');
                meta.style.fontSize = '10px';
                meta.style.color = 'var(--text-muted)';
                meta.textContent = window.vt("{0} • {1} MB • 🏆 {2}점", item.file_type, (item.file_size / (1024 * 1024)).toFixed(1), item.score || 1000);

                const actions = document.createElement('div');
                actions.className = 'trash-card-actions';

                const restoreBtn = document.createElement('button');
                restoreBtn.className = 'btn secondary-btn';
                restoreBtn.textContent = window.vt("↩️ 복원");
                restoreBtn.disabled=!!trashMutation;
                restoreBtn.onclick = () => {if(isFresh(scope))restoreFromTrash(item.id);};

                const deleteBtn = document.createElement('button');
                deleteBtn.className = 'btn danger-btn';
                deleteBtn.textContent = window.vt("❌ 영구삭제");
                deleteBtn.disabled=!!trashMutation;
                deleteBtn.onclick = () => {if(isFresh(scope))permanentlyDelete(item.id);};

                actions.appendChild(restoreBtn);
                actions.appendChild(deleteBtn);

                info.appendChild(title);
                info.appendChild(meta);
                info.appendChild(actions);

                card.appendChild(thumb);
                card.appendChild(info);
                trashItemsGrid.appendChild(card);
            });
        } catch (err) {
            console.error('Failed to load trash items:', err);
        }
    }

    function updateTrashLogControls() {
        const disabled = deletingTrashLog || !trashLogsLoaded;
        btnClearTrashLogs.disabled = disabled || Number(logsCountEl.textContent) === 0;
        trashLogsTbody.querySelectorAll('button').forEach(button => { button.disabled = disabled; });
    }

    async function deleteTrashLog(log = null, scope = captureScope()) {
        if (!isFresh(scope) || deletingTrashLog || !trashLogsLoaded) return;
        if (!log && Number(logsCountEl.textContent) === 0) return;
        const message = log
            ? window.vt("“{0}”의 삭제 이력을 지우시겠습니까?", log.original_rel_path)
            : window.vt("현재 폴더의 삭제 이력 {0}개를 모두 지우시겠습니까?", logsCountEl.textContent);
        if (!confirm(window.vt("{0}\n이력 삭제는 되돌릴 수 없습니다. 실제 파일과 휴지통 항목은 유지됩니다.", message))) return;
        deletingTrashLog = true;
        trashLogsLoadVersion += 1; // Older list responses must not restore deleted rows.
        updateTrashLogControls();
        trashLogsStatus.textContent = window.vt("이력을 삭제하는 중…");
        try {
            const path = log ? `/api/trash/logs/${log.id}` : '/api/trash/logs';
            await vaultFetch(path, { method: 'DELETE', scope, discardBody:true });
            if (!isFresh(scope)) return;
            await loadTrashLogs(scope);
        } catch (err) {
            if (isFresh(scope) && err.name !== 'AbortError') {
                trashLogsStatus.textContent = window.vt("이력을 삭제하지 못했습니다: {0}", err.message);
            }
        } finally {
            if (isFresh(scope)) {
                deletingTrashLog = false;
                updateTrashLogControls();
            }
        }
    }

    async function loadTrashLogs(scope = captureScope()) {
        const version = ++trashLogsLoadVersion;
        trashLogsLoaded = false;
        updateTrashLogControls();
        trashLogsStatus.textContent = window.vt("삭제 이력을 불러오는 중…");
        try {
            const res = await vaultFetch('/api/trash/logs', { scope });
            const logs = await res.json();
            if (!isFresh(scope) || version !== trashLogsLoadVersion) return;
            logsCountEl.textContent = logs.length;
            trashLogsLoaded = true;
            trashLogsStatus.textContent = '';
            trashLogsTbody.replaceChildren();
            if (logs.length === 0) {
                trashLogsTbody.innerHTML = window.vt("<tr><td colspan=\"8\" style=\"text-align: center; color: var(--text-muted); padding: 20px;\">삭제 이력 기록이 없습니다.</td></tr>");
            }
            logs.forEach(log => {
                const tr = document.createElement('tr');
                // File names and tags are plain text, including HTML-like names.
                const values = [log.id, log.original_rel_path, log.file_type,
                    `${(log.file_size / (1024 * 1024)).toFixed(1)} MB`, window.vt("🏆 {0}점", log.score),
                    log.tags_info ? '#' + log.tags_info : '-', log.deleted_time_str || '-'];
                values.forEach((value, index) => {
                    const td = document.createElement('td');
                    td.textContent = value;
                    if (index === 1) td.title = log.original_rel_path;
                    tr.appendChild(td);
                });
                const actions = document.createElement('td');
                const button = document.createElement('button');
                button.className = 'btn secondary-btn delete-btn';
                button.textContent = window.vt("이력 삭제");
                button.title = window.vt("{0} 이력 삭제", log.original_rel_path);
                button.onclick = () => deleteTrashLog(log, scope);
                actions.appendChild(button);
                tr.appendChild(actions);
                trashLogsTbody.appendChild(tr);
            });
        } catch (err) {
            if (isFresh(scope) && version === trashLogsLoadVersion && err.name !== 'AbortError') {
                trashLogsStatus.textContent = window.vt("삭제 이력을 불러오지 못했습니다: {0}", err.message);
            }
        } finally {
            if (isFresh(scope) && version === trashLogsLoadVersion) updateTrashLogControls();
        }
    }

    function openTrash() {
        pdfViewer.reset();
        watchSession = null;
        document.getElementById('note-stage').hidden=true;
        inTrashMode = true;
        inArenaMode = false;
        video.pause();
        videoStage.style.display = 'none';
        imageStage.style.display = 'none';
        arenaStage.style.display = 'none';
        trashStage.style.display = 'flex';

        switchTrashTab('items');
    }

    function exitTrash() {
        inTrashMode = false;
        trashStage.style.display = 'none';
        loadMediaFiles();
        if (currentFile) {
            selectMedia(currentFile);
        } else if (filesData.length > 0) {
            selectMedia(filesData[0]);
        }
    }

    function switchTrashTab(tab) {
        if (tab === 'items') {
            tabTrashItems.classList.add('active');
            tabTrashLogs.classList.remove('active');
            trashItemsContent.style.display = 'flex';
            trashLogsContent.style.display = 'none';
            loadTrashItems();
        } else {
            tabTrashItems.classList.remove('active');
            tabTrashLogs.classList.add('active');
            trashItemsContent.style.display = 'none';
            trashLogsContent.style.display = 'flex';
            loadTrashLogs();
        }
    }

    btnOpenTrash.addEventListener('click', openTrash);
    btnExitTrash.addEventListener('click', exitTrash);
    tabTrashItems.addEventListener('click', () => switchTrashTab('items'));
    tabTrashLogs.addEventListener('click', () => switchTrashTab('logs'));
    btnEmptyTrash.addEventListener('click', emptyTrash);
    btnClearTrashLogs.addEventListener('click', () => deleteTrashLog());

    if (btnTrashCurrentVideo) {
        btnTrashCurrentVideo.addEventListener('click', () => {
            if (currentFile) moveToTrash(currentFile.id);
        });
    }

    if (btnTrashCurrentImage) {
        btnTrashCurrentImage.addEventListener('click', () => {
            if (currentFile) moveToTrash(currentFile.id);
        });
    }

    // ==========================================
    // Media Arena System (A vs B Elo Match)
    // ==========================================
    btnStartArena.addEventListener('click', () => startArena());
    btnExitArena.addEventListener('click', exitArena);

    arenaCardA.addEventListener('click', () => {
        if (currentArenaPair && !isVoting) {
            handleVote(currentArenaPair[0].id, currentArenaPair[1].id, 'A');
        }
    });

    arenaCardB.addEventListener('click', () => {
        if (currentArenaPair && !isVoting) {
            handleVote(currentArenaPair[1].id, currentArenaPair[0].id, 'B');
        }
    });

    if (btnArenaPass) {
        btnArenaPass.addEventListener('click', (e) => {
            e.stopPropagation();
            handleArenaPass();
        });
    }

    if (btnArenaPassTop) {
        btnArenaPassTop.addEventListener('click', (e) => {
            e.stopPropagation();
            handleArenaPass();
        });
    }

    function startArena(pinnedId = null) {
        pdfViewer.reset();
        // PointerEvent나 MouseEvent 등 이벤트 객체 전달 방지 및 올바른 숫자 ID 검증
        const validPinnedId = (typeof pinnedId === 'number' || (typeof pinnedId === 'string' && /^\d+$/.test(pinnedId))) ? Number(pinnedId) : null;
        watchSession = null;
        document.getElementById('note-stage').hidden=true;
        inArenaMode = true;
        inTrashMode = false;
        video.pause();
        videoStage.style.display = 'none';
        imageStage.style.display = 'none';
        trashStage.style.display = 'none';
        arenaStage.style.display = 'flex';
        arenaPinnedMediaId = validPinnedId;
        applySearchAndRender();
        loadArenaPair();
    }

    function exitArena() {
        arenaLoadVersion++; currentArenaPair = null; setArenaBusy(false);
        stopArenaVideos();
        resetArenaArchiveStates();
        inArenaMode = false;
        arenaPinnedMediaId = null;
        arenaStage.style.display = 'none';
        loadMediaFiles();
        if (currentFile) {
            selectMedia(currentFile);
        } else if (filesData.length > 0) {
            selectMedia(filesData[0]);
        }
    }

    function updateArenaPinUI() {
        if (arenaPinnedMediaId && currentArenaPair && currentArenaPair[0]) {
            if (arenaPinnedBadge) arenaPinnedBadge.style.display = 'flex';
            if (arenaPinnedText) {
                const shortName = currentArenaPair[0].series_name || mediaName(currentArenaPair[0]).split('/').pop();
                arenaPinnedText.textContent = window.vt("📌 좌측 고정: {0}", shortName);
                arenaPinnedText.title = mediaName(currentArenaPair[0]);
            }
            if (btnPinCardA) {
                btnPinCardA.classList.add('pinned');
                btnPinCardA.innerHTML = window.vt("📌 고정 해제");
                btnPinCardA.title = window.vt("좌측 고정을 해제하고 일반 랜덤 대결로 전환");
            }
        } else {
            if (arenaPinnedBadge) arenaPinnedBadge.style.display = 'none';
            if (btnPinCardA) {
                btnPinCardA.classList.remove('pinned');
                btnPinCardA.innerHTML = window.vt("📌 좌측 고정");
                btnPinCardA.title = window.vt("이 미디어를 좌측에 고정하고 상대 미디어만 바꿔가며 연속 대결");
            }
        }
    }

    if (btnPinCardA) {
        btnPinCardA.addEventListener('click', (e) => {
            e.stopPropagation();
            if (arenaPinnedMediaId) {
                arenaPinnedMediaId = null;
                updateArenaPinUI();
                loadArenaPair();
            } else if (currentArenaPair && currentArenaPair[0]) {
                arenaPinnedMediaId = currentArenaPair[0].id;
                updateArenaPinUI();
                loadArenaPair();
            }
        });
    }

    if (btnUnpinArena) {
        btnUnpinArena.addEventListener('click', (e) => {
            e.stopPropagation();
            arenaPinnedMediaId = null;
            updateArenaPinUI();
            loadArenaPair();
        });
    }

    let arenaArchiveStateA = { fileId: null, entries: null, currentIdx: 0, loading: false };
    let arenaArchiveStateB = { fileId: null, entries: null, currentIdx: 0, loading: false };
    let arenaWheelCooldown = false;

    function resetArenaArchiveStates() {
        arenaArchiveStateA = { fileId: null, entries: null, currentIdx: 0, loading: false };
        arenaArchiveStateB = { fileId: null, entries: null, currentIdx: 0, loading: false };
        if (arenaPageA) {
            arenaPageA.style.display = 'none';
            arenaPageA.textContent = '';
        }
        if (arenaPageB) {
            arenaPageB.style.display = 'none';
            arenaPageB.textContent = '';
        }
    }

    function stopArenaVideos() {
        if (arenaVideoA) {
            arenaVideoA.pause();
            arenaVideoA.currentTime = 0;
            arenaVideoA.removeAttribute('src');
            arenaVideoA.load();
            arenaVideoA.style.display = 'none';
        }
        if (arenaVideoB) {
            arenaVideoB.pause();
            arenaVideoB.currentTime = 0;
            arenaVideoB.removeAttribute('src');
            arenaVideoB.load();
            arenaVideoB.style.display = 'none';
        }
    }

    function playArenaVideoPreview(videoEl, item) {
        if (!videoEl || !item || item.file_type !== 'VIDEO') return;
        const encodedPath = encodeMediaPath(item.rel_path);
        const targetSrc = mediaUrl(`/api/media/${encodedPath}`);
        if (!videoEl.src.endsWith(targetSrc)) {
            videoEl.src = targetSrc;
        }
        videoEl.muted = true;
        videoEl.style.display = 'block';
        const startTime = (typeof item.time_offset === 'number' && !isNaN(item.time_offset) && item.time_offset >= 0)
            ? item.time_offset
            : 0;
        videoEl.currentTime = startTime;
        videoEl.play().catch(err => console.log('Arena video preview play error:', err));
    }

    function stopArenaVideoPreview(videoEl, item) {
        if (!videoEl) return;
        videoEl.pause();
        const startTime = (item && typeof item.time_offset === 'number' && !isNaN(item.time_offset) && item.time_offset >= 0)
            ? item.time_offset
            : 0;
        videoEl.currentTime = startTime;
        videoEl.style.display = 'none';
    }

    async function handleArenaArchiveWheel(e, side) {
        const item = (currentArenaPair && side === 'A') ? currentArenaPair[0] : (currentArenaPair ? currentArenaPair[1] : null);
        if (!item || item.file_type !== 'ARCHIVE' || e.deltaY === 0) return;

        e.preventDefault();
        e.stopPropagation();

        if (arenaWheelCooldown) return;
        arenaWheelCooldown = true;
        setTimeout(() => { arenaWheelCooldown = false; }, 120);

        const state = (side === 'A') ? arenaArchiveStateA : arenaArchiveStateB;
        const version = arenaLoadVersion;
        const fresh = () => inArenaMode && version === arenaLoadVersion &&
            state === (side === 'A' ? arenaArchiveStateA : arenaArchiveStateB) &&
            item === currentArenaPair?.[side === 'A' ? 0 : 1];
        const elThumb = (side === 'A') ? arenaThumbA : arenaThumbB;
        const elPage = (side === 'A') ? arenaPageA : arenaPageB;
        const encodedPath = encodeMediaPath(item.rel_path);
        const archiveExt = item.rel_path.split('.').pop().toLowerCase();
        if (archiveExt === 'rar' || archiveExt === '7z') {
            if (elPage) { elPage.textContent = window.vt("RAR/7z 미지원"); elPage.style.display = 'block'; }
            return;
        }

        if (state.fileId !== item.id || !state.entries) {
            if (state.loading) return;
            state.loading = true;
            try {
                const res = await vaultFetch(`/api/archive/list/${encodedPath}`);
                const order = buildArchiveOrder(await res.json());
                if (!fresh()) return;
                state.entries = order.entries;
                state.fileId = item.id;
                state.currentIdx = order.storedToDisplay[item.page_offset] ?? 0;
            } catch (err) {
                console.error('Failed to fetch arena archive list:', err);
                state.loading = false;
                return;
            }
            state.loading = false;
        }

        if (!fresh() || !state.entries || state.entries.length === 0) return;

        if (e.deltaY > 0) {
            if (state.currentIdx < state.entries.length - 1) {
                state.currentIdx++;
            }
        } else {
            if (state.currentIdx > 0) {
                state.currentIdx--;
            }
        }

        const entry = state.entries[state.currentIdx];
        elThumb.src = mediaUrl(`/api/archive/file/${encodedPath}`, { entry });
        elThumb.style.display = 'block';

        if (elPage) {
            elPage.textContent = `📖 ${state.currentIdx + 1} / ${state.entries.length}`;
            elPage.style.display = 'block';
        }
    }

    if (arenaThumbWrapperA) {
        arenaThumbWrapperA.addEventListener('mouseenter', () => {
            if (currentArenaPair && currentArenaPair[0]) {
                playArenaVideoPreview(arenaVideoA, currentArenaPair[0]);
            }
        });
        arenaThumbWrapperA.addEventListener('mouseleave', () => {
            stopArenaVideoPreview(arenaVideoA, currentArenaPair ? currentArenaPair[0] : null);
        });
        arenaThumbWrapperA.addEventListener('wheel', (e) => handleArenaArchiveWheel(e, 'A'), { passive: false });
    }

    if (arenaThumbWrapperB) {
        arenaThumbWrapperB.addEventListener('mouseenter', () => {
            if (currentArenaPair && currentArenaPair[1]) {
                playArenaVideoPreview(arenaVideoB, currentArenaPair[1]);
            }
        });
        arenaThumbWrapperB.addEventListener('mouseleave', () => {
            stopArenaVideoPreview(arenaVideoB, currentArenaPair ? currentArenaPair[1] : null);
        });
        arenaThumbWrapperB.addEventListener('wheel', (e) => handleArenaArchiveWheel(e, 'B'), { passive: false });
    }

    function renderArenaItem(elThumb, elFallback, elType, elTitle, elScore, elDelta, elPage, item) {
        if (item.thumbnail_data) {
            elThumb.src = item.thumbnail_data.startsWith('/api/') ? mediaUrl(item.thumbnail_data) : item.thumbnail_data;
            elThumb.style.display = 'block';
            if (elFallback) elFallback.style.display = 'none';
        } else {
            const encodedPath = encodeMediaPath(item.rel_path);
            if (item.file_type === 'IMAGE') {
                elThumb.src = mediaUrl(`/api/media/${encodedPath}`);
                elThumb.style.display = 'block';
                if (elFallback) elFallback.style.display = 'none';
            } else {
                elThumb.src = '';
                elThumb.style.display = 'none';
                if (elFallback) {
                    elFallback.textContent = item.file_type === 'VIDEO' ? '🎥' : (item.file_type === 'ARCHIVE' ? '📦' : item.file_type === 'PDF' ? '📄' : '🖼️');
                    elFallback.style.display = 'flex';
                }
            }
        }

        const representativeLabel = item.file_type === 'VIDEO' ? window.vt("대표 영상") : window.vt("대표 권");
        elType.textContent = (item.series_id ? window.vt("📚 시리즈 · {0}", representativeLabel) : (archiveUnsupported(item) ? window.vt("RAR/7z 미지원") : item.file_type)) + (item.encrypted ? window.vt(" · 🔒 암호화") : '');
        elTitle.textContent = item.series_name || mediaName(item);
        elTitle.title = item.series_id ? `${representativeLabel}: ${mediaName(item)}` : mediaName(item);
        elType.title = item.series_id ? `${representativeLabel}: ${mediaName(item)}${archiveUnsupported(item) ? window.vt(" (RAR/7z 미지원)") : ''}` : item.file_type;
        elScore.textContent = item.score ?? 1000;
        elScore.title = item.series_id ? window.vt("시리즈 공통 점수") : window.vt("미디어 점수");
        elDelta.textContent = '';
        elDelta.className = 'arena-delta';

        if (elPage) {
            if (item.file_type === 'ARCHIVE' && typeof item.page_offset === 'number') {
                elPage.textContent = window.vt("📖 저장한 페이지");
                elPage.style.display = 'block';
            } else {
                elPage.style.display = 'none';
            }
        }
    }

    function setArenaBusy(value) {
        isVoting = value;
        for (const button of [btnArenaPass, btnArenaPassTop, btnPinCardA, btnUnpinArena]) if (button) button.disabled = value;
        for (const card of [arenaCardA, arenaCardB]) card.setAttribute('aria-disabled', String(value || !currentArenaPair));
    }

    async function loadArenaPair(excludeIds = []) {
        const loadVersion = ++arenaLoadVersion;
        const scope = captureScope();
        currentArenaPair = null; setArenaBusy(true);
        stopArenaVideos();
        resetArenaArchiveStates();
        try {
            const url = arenaPinnedMediaId ? `/api/arena/pair?focus_id=${arenaPinnedMediaId}` : '/api/arena/pair';
            let pair = null;

            // 동일한 매치 재등장 방지 (최대 3회 재시도)
            for (let attempt = 0; attempt < 3; attempt++) {
                const res = await vaultFetch(url);
                if (!res.ok) {
                    console.error(`Failed to load arena pair: ${res.status} ${res.statusText}`);
                    return;
                }
                pair = await res.json();
                if (!pair || pair.length !== 2) break;
                if (!excludeIds || excludeIds.length < 2) break;

                // 고정 모드인 경우 상대방(pair[1])이 달라졌는지 확인
                if (arenaPinnedMediaId) {
                    if (pair[1].id !== excludeIds[1]) break;
                } else {
                    // 일반 모드인 경우 두 아이템 쌍이 이전과 동일하지 않은지 확인
                    const isSamePair = (pair[0].id === excludeIds[0] && pair[1].id === excludeIds[1]) ||
                                       (pair[0].id === excludeIds[1] && pair[1].id === excludeIds[0]);
                    if (!isSamePair) break;
                }
            }

            if (!isFresh(scope) || loadVersion !== arenaLoadVersion || !inArenaMode) return;
            if (pair && pair.length === 2) {
                currentArenaPair = pair;
                renderArenaItem(arenaThumbA, arenaFallbackA, arenaTypeA, arenaTitleA, arenaScoreA, arenaDeltaA, arenaPageA, pair[0]);
                renderArenaItem(arenaThumbB, arenaFallbackB, arenaTypeB, arenaTitleB, arenaScoreB, arenaDeltaB, arenaPageB, pair[1]);
                updateArenaPinUI();
            } else {
                alert(window.vt("아레나에는 두 개 이상의 참가 대상이 필요합니다. 시리즈는 대표 파일 하나만 참가하며, 대표 파일이 없거나 휴지통에 있으면 제외됩니다."));
                exitArena();
            }
        } catch (err) {
            console.error('Failed to load arena pair:', err);
        } finally {
            if (isFresh(scope) && loadVersion === arenaLoadVersion && inArenaMode) setArenaBusy(false);
        }
    }

    async function handleArenaPass() {
        const scope = captureScope(), version = arenaLoadVersion;
        if (isVoting || !inArenaMode) return;
        setArenaBusy(true);
        stopArenaVideos();
        resetArenaArchiveStates();

        if (arenaDeltaA) {
            arenaDeltaA.textContent = 'PASS';
            arenaDeltaA.className = 'arena-delta pass';
        }
        if (arenaDeltaB) {
            arenaDeltaB.textContent = 'PASS';
            arenaDeltaB.className = 'arena-delta pass';
        }

        const prevIds = currentArenaPair ? currentArenaPair.map(item => item.id) : [];

        setTimeout(async () => {
            if (!isFresh(scope) || !inArenaMode || version !== arenaLoadVersion) return;
            await loadArenaPair(prevIds);
        }, 220);
    }

    async function handleVote(winnerId, loserId, winnerSide) {
        const scope = captureScope(), version = arenaLoadVersion;
        const fresh = () => isFresh(scope) && inArenaMode && version === arenaLoadVersion;
        if (isVoting || !inArenaMode || !currentArenaPair ||
            !currentArenaPair.some(item => item.id === winnerId) || !currentArenaPair.some(item => item.id === loserId)) return;
        setArenaBusy(true);
        stopArenaVideos();
        resetArenaArchiveStates();

        try {
            const res = await vaultFetch('/api/arena/vote', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ winner_id: winnerId, loser_id: loserId })
            });
            const data = await res.json();

            if (!fresh()) return;
            if (winnerSide === 'A') {
                arenaScoreA.textContent = data.winner_score;
                arenaDeltaA.textContent = `+${data.winner_delta}`;
                arenaDeltaA.className = 'arena-delta plus';

                arenaScoreB.textContent = data.loser_score;
                arenaDeltaB.textContent = `${data.loser_delta}`;
                arenaDeltaB.className = 'arena-delta minus';
            } else {
                arenaScoreB.textContent = data.winner_score;
                arenaDeltaB.textContent = `+${data.winner_delta}`;
                arenaDeltaB.className = 'arena-delta plus';

                arenaScoreA.textContent = data.loser_score;
                arenaDeltaA.textContent = `${data.loser_delta}`;
                arenaDeltaA.className = 'arena-delta minus';
            }

            await loadMediaFiles();
            if (!fresh()) return;
            setTimeout(() => {
                if (!fresh()) return;
                loadArenaPair();
            }, 550);
        } catch (err) {
            console.error('Failed to vote in arena:', err);
            if (fresh()) setArenaBusy(false);
        }
    }

    // ==========================================
    // Player Controls & Volume
    // ==========================================
    function togglePlay() {
        if (!video.src || video.src === window.location.href) return;
        if (video.paused) {
            video.play();
        } else {
            video.pause();
        }
    }

    btnPlayPause.addEventListener('click', togglePlay);

    // 영상 영역 클릭 시 마우스의 화면 X축 위치와 일치하는 재생 시점으로 이동
    videoWrapper.addEventListener('click', (e) => {
        if (!video.duration || isNaN(video.duration)) return;
        if (!seekBarWrapper) return;
        const { ratio } = getPointerProgress(seekBarWrapper, e.clientX);
        video.currentTime = ratio * video.duration;
    });

    // 영상 영역 마우스 이동 시 재생바에 마우스 커서와 수직으로 정확히 일치하는 위치 및 시간 표시
    videoWrapper.addEventListener('mousemove', (e) => {
        if (!video.duration || isNaN(video.duration)) return;
        if (!seekBarWrapper) return;
        const { x, ratio } = getPointerProgress(seekBarWrapper, e.clientX);
        const hoverTime = ratio * video.duration;
        showHoverMarker(seekHoverIndicator, seekHoverTime, x, formatTime(hoverTime));
    });

    videoWrapper.addEventListener('mouseleave', () => {
        hideHoverMarker(seekHoverIndicator, seekHoverTime);
    });

    // 재생바 영역 자체 마우스 호버 처리
    if (seekBarWrapper) {
        seekBarWrapper.addEventListener('mousemove', (e) => {
            if (!video.duration || isNaN(video.duration)) return;
            const { x, ratio } = getPointerProgress(seekBarWrapper, e.clientX);
            const hoverTime = ratio * video.duration;
            showHoverMarker(seekHoverIndicator, seekHoverTime, x, formatTime(hoverTime));
        });

        seekBarWrapper.addEventListener('mouseleave', () => {
            hideHoverMarker(seekHoverIndicator, seekHoverTime);
        });
    }

    video.addEventListener('play', () => {
        btnPlayPause.textContent = '⏸';
    });

    video.addEventListener('pause', () => {
        stopVideoWatch();
        btnPlayPause.textContent = '▶';
    });

    video.addEventListener('playing', startVideoWatch);
    video.addEventListener('waiting', () => stopVideoWatch());
    video.addEventListener('seeking', () => stopVideoWatch(false));
    video.addEventListener('seeked', () => { if (video.readyState >= 3) startVideoWatch(); });
    video.addEventListener('ended', () => stopVideoWatch());
    video.addEventListener('emptied', () => stopVideoWatch(false));
    video.addEventListener('error', () => stopVideoWatch(false));
    video.addEventListener('ratechange', sampleVideoWatch);

    video.addEventListener('timeupdate', () => {
        if (!video.paused && !video.seeking) sampleVideoWatch();
        if (video.duration) {
            const pct = (video.currentTime / video.duration) * 100;
            seekBar.value = pct;
            currentTimeEl.textContent = formatTime(video.currentTime);
            durationEl.textContent = formatTime(video.duration);
        }
    });

    seekBar.addEventListener('input', () => {
        if (video.duration) {
            video.currentTime = (seekBar.value / 100) * video.duration;
        }
    });

    // ==========================================
    // Volume Control & HUD
    // ==========================================
    let volumeHudTimer = null;
    function updateVolumeUI(vol, showHud = true) {
        const clamped = Math.max(0, Math.min(1, Math.round(vol * 100) / 100));
        video.volume = clamped;
        if (clamped > 0) {
            video.muted = false;
        } else {
            video.muted = true;
        }
        volumeBar.value = clamped;
        btnMute.textContent = (clamped === 0 || video.muted) ? '🔇' : '🔊';

        if (showHud && volumeHud && volumeHudIcon && volumeHudText && volumeHudFill) {
            const pct = Math.round(clamped * 100);
            const icon = clamped === 0 ? '🔇' : (clamped < 0.5 ? '🔉' : '🔊');
            volumeHudIcon.textContent = icon;
            volumeHudText.textContent = `${pct}%`;
            volumeHudFill.style.width = `${pct}%`;
            volumeHud.classList.add('visible');
            if (volumeHudTimer) clearTimeout(volumeHudTimer);
            volumeHudTimer = setTimeout(() => {
                volumeHud.classList.remove('visible');
            }, 900);
        }
    }

    let videoWheelState = {selection:-1,delta:0,lastEvent:-Infinity,lastSeek:-Infinity};
    function handleVideoWheel(e) {
        if (videoStage.style.display === 'none' || currentFile?.file_type !== 'VIDEO') return;
        if (!video.src || video.src === window.location.href || !e.deltaY) return;
        if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey || Math.abs(e.deltaX || 0)>Math.abs(e.deltaY)) return;
        if (e.target?.closest('.volume-container')) {
            e.preventDefault(); e.stopPropagation();
            updateVolumeUI((video.muted?0:video.volume)+(e.deltaY<0?.05:-.05),true);
            return;
        }
        if (e.target?.closest('.bookmark-overlay,#video-bookmarks,button,select,input:not(#seek-bar)')) return;
        if (!Number.isFinite(video.duration) || video.duration<=0 || !Number.isFinite(video.currentTime)) return;
        e.preventDefault(); e.stopPropagation();
        const now=performance.now();
        if (videoWheelState.selection!==selectionVersion) videoWheelState={selection:selectionVersion,delta:0,lastEvent:-Infinity,lastSeek:-Infinity};
        const state=videoWheelState;
        if(now-state.lastEvent>200 || Math.sign(state.delta)!==Math.sign(e.deltaY))state.delta=0;
        state.lastEvent=now;
        if(now-state.lastSeek<120){state.delta=0;return;}
        state.delta+=e.deltaY*(e.deltaMode===1?16:e.deltaMode===2?400:1);
        if(Math.abs(state.delta)<40)return;
        const amount=state.delta>0?5:-5;state.delta=0;state.lastSeek=now;
        seekVideo(amount);
        seekBar.value=video.currentTime/video.duration*100;
        currentTimeEl.textContent=formatTime(video.currentTime);
    }

    videoWrapper.addEventListener('wheel', handleVideoWheel, { passive: false });
    videoStage.addEventListener('wheel', handleVideoWheel, { passive: false });

    // 마우스 휠로 ZIP/압축파일 이미지 넘기기 (아래로: 다음장, 위로: 이전장)
    function handleImageWheel(e) {
        if (imageStage.style.display === 'none') return;
        if (!currentFile || !['ARCHIVE','IMAGE'].includes(currentFile.file_type)) return;
        if (e.target?.closest('.bookmark-overlay')) return;
        if (e.deltaY === 0) return;

        e.preventDefault();
        e.stopPropagation();

        if (imageWheelCooldown) return;
        imageWheelCooldown = true;
        setTimeout(() => {
            imageWheelCooldown = false;
        }, 160);

        if (e.deltaY > 0) {
            nextArchiveImage();
        } else {
            prevArchiveImage();
        }
    }

    if (imageStage) imageStage.addEventListener('wheel', handleImageWheel, { passive: false });
    if (imageWrapper) imageWrapper.addEventListener('wheel', handleImageWheel, { passive: false });

    volumeBar.addEventListener('input', () => {
        updateVolumeUI(parseFloat(volumeBar.value), true);
    });

    btnMute.addEventListener('click', () => {
        if (video.volume > 0 && !video.muted) {
            updateVolumeUI(0, true);
        } else {
            updateVolumeUI(0.5, true);
        }
    });

    // 프로그램 실행 시 기본 볼륨을 0(음소거)으로 초기화
    updateVolumeUI(0, false);

    speedSelect.addEventListener('change', () => {
        video.playbackRate = parseFloat(speedSelect.value);
    });

    // One registry powers both dispatch and the help list, so labels stay in sync.
    const shortcutDialog = document.getElementById('shortcut-dialog');
    const shortcutList = document.getElementById('shortcut-list');
    const btnShortcuts = document.getElementById('btn-shortcuts');
    function showShortcuts() {
        if (!shortcutDialog.open) shortcutDialog.showModal();
    }
    btnShortcuts.addEventListener('click', showShortcuts);
    document.getElementById('btn-close-shortcuts').addEventListener('click', () => shortcutDialog.close());
    const browsingMedia = () => !inArenaMode && !inTrashMode && !!currentFile;
    const playingVideo = () => browsingMedia() && currentFile.file_type === 'VIDEO';
    const readingPdf = () => browsingMedia() && currentFile.file_type === 'PDF';
    const readingImage = () => browsingMedia() && currentFile.file_type === 'IMAGE';
    const readingArchive = () => browsingMedia() && currentFile.file_type === 'ARCHIVE' && archiveEntries.length > 0;
    const clickEnabled = button => { if (button && !button.disabled && !button.hidden) button.click(); };
    const combo = (code, modifiers = '') => e => e.code === code &&
        !!e.ctrlKey === modifiers.includes('C') && !!e.shiftKey === modifiers.includes('S') &&
        !!e.altKey === modifiers.includes('A') && !e.metaKey;
    const key = (...matchers) => e => matchers.some(match => match(e));
    const seekVideo = amount => {
        if (Number.isFinite(video.duration)) video.currentTime = Math.max(0, Math.min(video.duration, video.currentTime + amount));
    };
    const changeSpeed = step => {
        const values = Array.from(speedSelect.options, option => Number(option.value));
        const index = values.indexOf(Number(speedSelect.value));
        speedSelect.value = speedSelect.options[Math.max(0, Math.min(values.length - 1, index + step))].value;
        video.playbackRate = Number(speedSelect.value);
    };
    const shortcuts = [
        { group:window.vt("공통"), keys:'F1 / ?', description:window.vt("단축키 전체 목록"), match:key(combo('F1'), combo('Slash', 'S')), run:showShortcuts, editable:true },
        { group:window.vt("공통"), keys:'Ctrl + F / /', description:window.vt("파일명 또는 시리즈 검색"), match:key(combo('KeyF', 'C'), combo('Slash')), run:() => { searchInput.focus(); searchInput.select(); }, editable:true },
        { group:window.vt("공통"), keys:'Ctrl + L', description:window.vt("라이브러리 폴더 선택"), match:combo('KeyL', 'C'), run:() => folderSelect.focus() },
        { group:window.vt("공통"), keys:'Ctrl + O', description:window.vt("새 라이브러리 폴더 지정"), match:combo('KeyO', 'C'), run:() => clickEnabled(btnAddFolder) },
        { group:window.vt("공통"), keys:'Ctrl + R', description:window.vt("현재 라이브러리 동기화"), match:combo('KeyR', 'C'), run:() => clickEnabled(btnRescanFolder) },
        { group:window.vt("공통"), keys:'Ctrl + Shift + B', description:window.vt("라이브러리 DB 백업"), match:combo('KeyB', 'CS'), run:() => clickEnabled(btnBackupDb) },
        { group:window.vt("공통"), keys:'Alt + A', description:window.vt("아레나 열기"), match:combo('KeyA', 'A'), run:() => { if (!inArenaMode) startArena(); } },
        { group:window.vt("공통"), keys:'Alt + T', description:window.vt("휴지통 열기"), match:combo('KeyT', 'A'), run:() => { if (!inTrashMode) openTrash(); } },
        { group:window.vt("공통"), keys:'Esc', description:window.vt("안내창 닫기 / 아레나·휴지통에서 돌아가기 / 검색 입력 해제"), match:combo('Escape'), run:() => { if (inArenaMode) exitArena(); else if (inTrashMode) exitTrash(); } },
        { group:window.vt("미디어"), keys:'↑ / ↓', description:window.vt("이전 / 다음 미디어"), match:key(combo('ArrowUp'), combo('ArrowDown')), when:browsingMedia, run:e => navigateMedia(e.code === 'ArrowUp' ? 'prev' : 'next') },
        { group:window.vt("미디어"), keys:'S', description:window.vt("현재 영상 위치 / 문서 페이지 저장"), match:combo('KeyS'), when:() => playingVideo() || readingArchive() || readingPdf(), run:() => readingPdf() ? pdfViewer.saveBookmark() : saveBookmark() },
        { group:window.vt("미디어"), keys:'C', description:window.vt("현재 화면을 대표 썸네일로 지정"), match:combo('KeyC'), when:browsingMedia, run:() => readingPdf() ? pdfViewer.captureThumbnail() : clickEnabled(playingVideo() ? btnCaptureThumb : btnCaptureImgThumb) },
        { group:window.vt("미디어"), keys:'0–5', description:window.vt("현재 미디어 별점 지정 (0: 해제)"), match:e => /^(Digit|Numpad)[0-5]$/.test(e.code) && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey, when:browsingMedia, run:e => updateFileRating(currentFile.id, Number(e.code.slice(-1))) },
        { group:window.vt("미디어"), keys:'Delete', description:window.vt("현재 미디어를 휴지통으로 이동 (확인 후)"), match:combo('Delete'), when:browsingMedia, run:() => { if (confirm(window.vt("현재 미디어를 휴지통으로 이동하시겠습니까?"))) moveToTrash(currentFile.id); } },
        { group:window.vt("영상"), keys:'Space', description:window.vt("재생 / 일시정지"), match:combo('Space'), when:playingVideo, run:togglePlay },
        { group:window.vt("영상"), keys:'← / →', description:window.vt("5초 뒤로 / 앞으로"), match:key(combo('ArrowLeft'), combo('ArrowRight')), when:playingVideo, run:e => seekVideo(e.code === 'ArrowLeft' ? -5 : 5), repeat:true },
        { group:window.vt("영상"), keys:window.vt("Shift + ↑ / ↓ 또는 Ctrl + ↑ / ↓"), description:window.vt("음량 5% 올리기 / 내리기"), match:key(combo('ArrowUp','S'), combo('ArrowDown','S'), combo('ArrowUp','C'), combo('ArrowDown','C')), when:playingVideo, run:e => updateVolumeUI((video.muted ? 0 : video.volume) + (e.code === 'ArrowUp' ? 0.05 : -0.05), true), repeat:true },
        { group:window.vt("영상"), keys:'M', description:window.vt("음소거 켜기 / 끄기"), match:combo('KeyM'), when:playingVideo, run:() => clickEnabled(btnMute) },
        { group:window.vt("영상"), keys:'[ / ]', description:window.vt("재생 속도 한 단계 낮추기 / 높이기"), match:key(combo('BracketLeft'), combo('BracketRight')), when:playingVideo, run:e => changeSpeed(e.code === 'BracketLeft' ? -1 : 1) },
        { group:window.vt("영상"), keys:'Home / End', description:window.vt("영상 시작 / 끝으로 이동"), match:key(combo('Home'), combo('End')), when:playingVideo, run:e => { if (Number.isFinite(video.duration)) video.currentTime = e.code === 'Home' ? 0 : video.duration; } },
        { group:window.vt("ZIP 이미지"), keys:'← / → / Space', description:window.vt("이전 / 다음 페이지 (처음·끝에서는 이전·다음 압축 파일)"), match:key(combo('ArrowLeft'), combo('ArrowRight'), combo('Space')), when:readingArchive, run:e => e.code === 'ArrowLeft' ? prevArchiveImage() : nextArchiveImage() },
        { group:window.vt("ZIP 이미지"), keys:'Home / End', description:window.vt("첫 페이지 / 마지막 페이지"), match:key(combo('Home'), combo('End')), when:readingArchive, run:e => showArchiveImage(encodeMediaPath(currentFile.rel_path), e.code === 'Home' ? 0 : archiveEntries.length - 1) },
        { group:window.vt("개별 이미지"), keys:'← / → / Space', description:window.vt("이전 / 다음 이미지"), match:key(combo('ArrowLeft'),combo('ArrowRight'),combo('Space')), when:readingImage, run:e => navigateImage(e.code === 'ArrowLeft' ? -1 : 1) },
        { group:'PDF', keys:'← / → / PageUp / PageDown / Space', description:window.vt("이전 / 다음 PDF 페이지"), match:key(combo('ArrowLeft'),combo('ArrowRight'),combo('PageUp'),combo('PageDown'),combo('Space')), when:readingPdf, run:e => pdfViewer.step(['ArrowLeft','PageUp'].includes(e.code) ? -1 : 1) },
        { group:'PDF', keys:'Home / End', description:window.vt("첫 페이지 / 마지막 페이지"), match:key(combo('Home'),combo('End')), when:readingPdf, run:e => pdfViewer.show(e.code === 'Home' ? 1 : pdfViewer.doc?.numPages) },
        { group:window.vt("아레나"), keys:'V / B', description:window.vt("왼쪽 / 오른쪽 미디어 선택"), match:key(combo('KeyV'), combo('KeyB')), when:() => inArenaMode, run:e => { if (currentArenaPair && !isVoting) { const side = e.code === 'KeyV' ? 0 : 1; handleVote(currentArenaPair[side].id, currentArenaPair[1-side].id, side === 0 ? 'A' : 'B'); } } },
        { group:window.vt("아레나"), keys:'P / Space', description:window.vt("현재 대결 패스"), match:key(combo('KeyP'), combo('Space')), when:() => inArenaMode, run:() => { if (currentArenaPair && !isVoting) handleArenaPass(); } },
        { group:window.vt("아레나"), keys:'F', description:window.vt("왼쪽 미디어 고정 / 해제"), match:combo('KeyF'), when:() => inArenaMode, run:() => { if (!isVoting) clickEnabled(btnPinCardA); } },
    ];
    for (const group of new Set(shortcuts.map(item => item.group))) {
        const section = document.createElement('section'); section.className = 'shortcut-group';
        const heading = document.createElement('h3'); heading.textContent = group; section.appendChild(heading);
        shortcuts.filter(item => item.group === group).forEach(item => {
            const row = document.createElement('div'); row.className = 'shortcut-row';
            const label = document.createElement('span'); label.textContent = item.description;
            const keys = document.createElement('kbd'); keys.textContent = item.keys;
            row.append(label, keys); section.appendChild(row);
        });
        shortcutList.appendChild(section);
    }
    function handleShortcut(e) {
        if(e.defaultPrevented)return;
        // Browser reload must never discard a draft, including inside a modal
        // or an input. Ctrl+R remains the app's library-sync shortcut below.
        if(e.code==='F5'||(e.code==='KeyR'&&(e.ctrlKey||e.metaKey))){
            e.preventDefault();
            if(e.code==='F5'||e.shiftKey||e.altKey||e.metaKey)return;
        }
        if (e.isComposing || e.keyCode === 229) return;
        if (window.VaultLicenses?.isOpen()) return;
        if (shortcutDialog.open) {
            if (e.key === 'Escape' || e.code === 'Escape') { e.preventDefault(); shortcutDialog.close(); }
            return;
        }
        if (privacy.covered || privacy.isOpen() || folderTransition || integrityCheck.isOpen() || nameEditor.isOpen() || noteEditor.isOpen() || pageEditor.isOpen() || seriesEditor.isOpen() || archiveSecurity.isOpen() || dbRestore.isOpen() || dbSecurity.isOpen() || dbSecurity.locked) return;
        const editable = !!e.target.closest?.('input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"]');
        if (editable && (e.target === searchInput || e.target === tagSearchInput) && e.key === 'Escape') {
            e.preventDefault(); e.target.blur(); return;
        }
        const shortcut = shortcuts.find(item => item.match(e) && (!item.when || item.when()));
        if (!shortcut) return;
        // F1 and Ctrl+F may be used from inputs; literal ? and / remain text.
        if (editable && !(shortcut.editable && (e.code === 'F1' || (e.code === 'KeyF' && e.ctrlKey)))) return;
        // Space/Enter retain native focused-button activation.
        if (e.target.closest?.('button, a[href], [role="button"]') && (e.code === 'Space' || e.code === 'Enter')) return;
        e.preventDefault();
        if (!e.repeat || shortcut.repeat) shortcut.run(e);
    }
    document.addEventListener('keydown', handleShortcut);

    // Manual Thumbnail Capture
    btnCaptureThumb.addEventListener('click', () => {
        if (currentFile && currentFile.file_type === 'VIDEO') {
            captureFrameAndUpload(currentFile.id);
        }
    });

    if (btnCaptureImgThumb) {
        btnCaptureImgThumb.addEventListener('click', () => {
            if (currentFile && (currentFile.file_type === 'ARCHIVE' || currentFile.file_type === 'IMAGE')) {
                captureImageThumbnail(currentFile.id);
            }
        });
    }

    function captureImageThumbnail(fileId) {
        const scope = captureScope();
        const selection = selectionVersion;
        const freshCapture = () => isFresh(scope) && selection === selectionVersion && currentFile?.id === fileId;
        if (!viewerImage.naturalWidth || !viewerImage.naturalHeight) return;

        const isDual = (viewerImage2 &&
                        viewerImage2.style.display !== 'none' &&
                        viewerImage2.naturalWidth > 0 &&
                        viewerImage2.naturalHeight > 0) ||
                       (imageDisplayBox &&
                        imageDisplayBox.classList.contains('dual') &&
                        viewerImage2 &&
                        viewerImage2.naturalWidth > 0);

        const ctx = canvas.getContext('2d');
        const pageOffset = archiveOrder.displayToStored[currentImageIdx] ?? 0;
        let captureWidth;
        let captureHeight;

        if (isDual) {
            // 2장 보기 모드: 좌/우 두 이미지를 스프레드로 나란히 합성
            const aspect1 = viewerImage.naturalWidth / viewerImage.naturalHeight;
            const aspect2 = viewerImage2.naturalWidth / viewerImage2.naturalHeight;
            const totalRelativeWidth = aspect1 + aspect2;

            const dualScale = Math.min(640, 640 / totalRelativeWidth);
            captureWidth = Math.max(1, Math.round(totalRelativeWidth * dualScale));
            captureHeight = Math.max(1, Math.round(dualScale));
            canvas.width = captureWidth;
            canvas.height = captureHeight;

            const w1 = Math.round(captureWidth * (aspect1 / totalRelativeWidth));
            const w2 = canvas.width - w1;

            ctx.drawImage(viewerImage, 0, 0, w1, canvas.height);
            ctx.drawImage(viewerImage2, w1, 0, w2, canvas.height);
            console.log(`[Thumbnail] Captured dual-page spread (${canvas.width}x${canvas.height}, w1: ${w1}, w2: ${w2}, page: ${pageOffset})`);
        } else {
            // 1장 보기 모드: 단일 이미지 비율 유지
            const scale = Math.min(640 / viewerImage.naturalWidth, 640 / viewerImage.naturalHeight);
            captureWidth = Math.max(1, Math.round(viewerImage.naturalWidth * scale));
            captureHeight = Math.max(1, Math.round(viewerImage.naturalHeight * scale));
            canvas.width = captureWidth;
            canvas.height = captureHeight;
            ctx.drawImage(viewerImage, 0, 0, captureWidth, captureHeight);
            console.log(`[Thumbnail] Captured single-page (${canvas.width}x${canvas.height}, page: ${pageOffset})`);
        }

        canvas.toBlob(async (blob) => {
            if (!blob || !freshCapture()) return;

            try {
                const formData = new FormData();
                formData.append('image', blob, 'thumb.webp');
                formData.append('width', captureWidth);
                formData.append('height', captureHeight);
                formData.append('page_offset', pageOffset);
                const res = await vaultFetch(`/api/thumbnail/${fileId}`, {
                    discardBody:true,
                    method: 'POST',
                    body: formData, scope
                });
                if (res.ok) {
                    if (!freshCapture()) return;
                    const dataUrl = `/api/thumbnail/${fileId}?v=${Date.now()}`;
                    updateThumbnailState(fileId, dataUrl, { page_offset: pageOffset });

                    btnCaptureImgThumb.textContent = isDual ? window.vt("✅ 2장 합본 저장 완료!") : window.vt("✅ 저장 완료!");
                    btnCaptureImgThumb.style.background = '#10b981';
                    imageCaptureResetTimer = setTimeout(() => {
                        imageCaptureResetTimer = null;
                        resetImageCaptureButton();
                    }, 1500);
                }
            } catch (err) {
                console.error('Failed to upload image thumbnail:', err);
            }
        }, 'image/webp', 0.85);
    }

    function captureFrameAndUpload(fileId) {
        const scope = captureScope();
        const selection = selectionVersion;
        const freshCapture = () => isFresh(scope) && selection === selectionVersion && currentFile?.id === fileId;
        if (!video.videoWidth || !video.videoHeight) return;

        const captureTime = video.currentTime || 0;
        const scale = Math.min(640 / video.videoWidth, 640 / video.videoHeight);
        const captureWidth = Math.max(1, Math.round(video.videoWidth * scale));
        const captureHeight = Math.max(1, Math.round(video.videoHeight * scale));
        canvas.width = captureWidth;
        canvas.height = captureHeight;
        const ctx = canvas.getContext('2d');
        ctx.drawImage(video, 0, 0, captureWidth, captureHeight);

        canvas.toBlob(async (blob) => {
            if (!blob || !freshCapture()) return;

            try {
                const formData = new FormData();
                formData.append('image', blob, 'thumb.webp');
                formData.append('width', captureWidth);
                formData.append('height', captureHeight);
                formData.append('time_offset', captureTime);
                const res = await vaultFetch(`/api/thumbnail/${fileId}`, {
                    discardBody:true,
                    method: 'POST',
                    body: formData, scope
                });
                if (res.ok) {
                    if (!freshCapture()) return;
                    const dataUrl = `/api/thumbnail/${fileId}?v=${Date.now()}`;
                    updateThumbnailState(fileId, dataUrl, { time_offset: captureTime });
                }
            } catch (err) {
                console.error('Failed to upload thumbnail:', err);
            }
        }, 'image/webp', 0.85);
    }

    function formatTime(seconds) {
        if (isNaN(seconds)) return '00:00';
        const mins = Math.floor(seconds / 60);
        const secs = Math.floor(seconds % 60);
        return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
    }

    async function initializeVaultSession(scope = captureScope()) {
        const res = await fetch('/api/session', {headers: {'X-Vault-Token': window.__valtheumToken || ''}});
        if (!res.ok) throw new Error('valtheum session unavailable');
        const session = await res.json();
        if (!isFresh(scope)) throw STALE_RESPONSE;
        if (vaultSession.generation !== session.generation) resetLibraryView();
        vaultSession = { token: window.__valtheumToken, generation: session.generation };
        adoptLibraryState(session);
    }

    document.querySelectorAll('[data-language-select]').forEach(select => {
        select.value = window.VaultI18n.language;
        select.addEventListener('change', async () => {
            const previous = window.VaultI18n.language;
            const checkNote=()=>{
                if(noteEditor.exitRisk())throw new Error(window.vt('메모 변경을 저장하거나 취소하고 진행 중인 메모 작업을 마친 뒤 언어를 변경해 주세요.'));
            };
            document.querySelectorAll('[data-language-select]').forEach(el => el.disabled = true);
            try {
                checkNote();
                await vaultFetch('/api/settings/language', {method:'POST',discardBody:true,headers:{'Content-Type':'application/json'},body:JSON.stringify({language:select.value})});
                checkNote();
                window.location.reload();
            } catch (error) {
                select.value = previous;
                alert(error.message);
            } finally {document.querySelectorAll('[data-language-select]').forEach(el => el.disabled = false);}
        });
    });

    // Initial load: keep all real UI hidden until the server confirms visibility.
    privacy.setCovered(true);
    (async () => {
        try {
            await initializeVaultSession();
            if(privacy.covered)return;
            await loadFolders();
            await loadMediaFiles();
            await updateTrashCount();
        } catch (err) {
            console.error('valtheum initialization failed:', err);
            if (mediaListEl) mediaListEl.textContent = window.vt("세션을 초기화할 수 없습니다. 앱을 다시 시작해 주세요.");
        }
    })();
});
