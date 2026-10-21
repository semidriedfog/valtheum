// Variable-height windowing: O(log n) offset lookup, bounded card DOM.
(() => {
    class HeightIndex {
        constructor(values) {
            this.values = values;
            this.tree = new Float64Array(values.length + 1);
            for (let i = 1; i < this.tree.length; i++) {
                this.tree[i] += values[i - 1];
                const parent = i + (i & -i);
                if (parent < this.tree.length) this.tree[parent] += this.tree[i];
            }
        }
        add(index, delta) {
            for (let i = index + 1; i < this.tree.length; i += i & -i) this.tree[i] += delta;
        }
        set(index, value) {
            this.add(index, value - this.values[index]);
            this.values[index] = value;
        }
        offset(end) {
            let sum = 0;
            for (let i = end; i > 0; i -= i & -i) sum += this.tree[i];
            return sum;
        }
        at(offset) {
            let index = 0, sum = 0;
            let step = 1;
            while (step * 2 < this.tree.length) step *= 2;
            for (; step > 0; step >>= 1) {
                const next = index + step;
                if (next < this.tree.length && sum + this.tree[next] <= offset) {
                    index = next; sum += this.tree[next];
                }
            }
            return Math.min(index, Math.max(0, this.values.length - 1));
        }
    }

    window.VirtualMediaList = class {
        constructor(element, createCard, decorateCard = () => {}) {
            this.element = element;
            this.createCard = createCard;
            this.decorateCard = decorateCard;
            this.items = [];
            this.indexById = new Map();
            this.heights = new Map();
            this.nodes = new Map();
            this.dirty = new Set();
            this.index = new HeightIndex([]);
            this.frame = null;
            this.gap = 5;
            this.estimate = 136;
            this.overscan = 5;
            this.width = element.clientWidth;
            this.canvas = document.createElement('div');
            this.canvas.className = 'media-virtual-canvas';
            element.replaceChildren(this.canvas);
            element.addEventListener('scroll', () => this.schedule(), { passive: true });
            element.addEventListener('focusout', () => this.schedule());
            for (const event of ['wheel', 'pointerdown', 'keydown']) {
                element.addEventListener(event, () => { this.pendingReveal = null; }, { passive: true });
            }
            if (typeof ResizeObserver !== 'undefined') {
                this.observer = new ResizeObserver(() => this.schedule());
                this.observer.observe(element);
            }
        }
        schedule() {
            if (this.frame !== null) return;
            this.frame = requestAnimationFrame(() => { this.frame = null; this.render(); });
        }
        focusedIndex() {
            for (const [index, card] of this.nodes) {
                if (card.contains(document.activeElement)) return index;
            }
            return -1;
        }
        anchor() {
            const index = this.index.at(this.element.scrollTop);
            return { id: this.items[index]?.id, index, inset: this.element.scrollTop - this.index.offset(index) };
        }
        restoreAnchor(anchor) {
            const index = this.indexById.get(anchor.id) ?? Math.min(anchor.index, this.items.length - 1);
            this.element.scrollTop = index < 0 ? 0 : Math.max(0, this.index.offset(index) + anchor.inset);
        }
        remove(index) {
            const card = this.nodes.get(index);
            this.observer?.unobserve(card);
            card.remove();
            this.nodes.delete(index);
            this.dirty.delete(index);
        }
        reset() {
            if (this.frame !== null) cancelAnimationFrame(this.frame);
            this.frame = null;
            this.heights.clear();
            this.pendingReveal = null;
            this.setItems([], { resetScroll: true });
        }
        setItems(items, { resetScroll = false, revealId = null } = {}) {
            const anchor = this.anchor();
            const focused = this.focusedIndex();
            const focusedId = this.items[focused]?.id;
            const focusedCard = this.nodes.get(focused);
            const nextIndex = new Map(items.map((file, index) => [file.id, index]));
            // Keep a focused tag editor attached, including during IME input.
            const keepFocus = !resetScroll && nextIndex.has(focusedId);
            for (const index of [...this.nodes.keys()]) {
                if (!keepFocus || index !== focused) this.remove(index);
            }
            this.nodes.clear(); this.dirty.clear();
            if (keepFocus) {
                const index = nextIndex.get(focusedId);
                this.nodes.set(index, focusedCard);
                this.dirty.add(index); // Refresh its metadata after focus leaves.
            }
            this.items = items;
            this.indexById = nextIndex;
            this.index = new HeightIndex(items.map(file => (this.heights.get(file.id) ?? this.estimate) + this.gap));
            this.canvas.style.height = `${this.totalHeight()}px`;
            if (resetScroll) this.element.scrollTop = 0;
            else this.restoreAnchor(anchor);
            this.pendingReveal = revealId;
            this.render();
        }
        reveal(id) {
            // Selection changes only decoration/scroll, not list contents or heights.
            this.pendingReveal = id;
            this.render();
        }
        totalHeight() { return Math.max(0, this.index.offset(this.items.length) - this.gap); }
        render() {
            if (!this.items.length) {
                this.canvas.style.height = 'auto';
                this.canvas.textContent = window.vt("검색 결과가 없습니다.");
                this.canvas.classList.add('empty');
                return;
            }
            if (this.canvas.classList.contains('empty')) {
                this.canvas.textContent = '';
                this.canvas.classList.remove('empty');
            }
            const anchor = this.anchor();
            let changed = false;
            if (this.element.clientWidth !== this.width) {
                this.width = this.element.clientWidth;
                this.heights.clear();
                this.index = new HeightIndex(this.items.map(() => this.estimate + this.gap));
                changed = true;
            }
            for (const [index, card] of this.nodes) {
                const height = card.getBoundingClientRect().height;
                if (height > 0 && Math.abs(this.index.values[index] - height - this.gap) > 0.5) {
                    this.index.set(index, height + this.gap);
                    this.heights.set(this.items[index].id, height);
                    changed = true;
                }
            }
            this.canvas.style.height = `${this.totalHeight()}px`;
            if (changed) this.restoreAnchor(anchor);
            const viewport = Math.max(1, this.element.clientHeight);
            const target = this.indexById.get(this.pendingReveal);
            if (target !== undefined) {
                const top = this.index.offset(target);
                const height = this.index.values[target] - this.gap;
                if (top < this.element.scrollTop || top + height > this.element.scrollTop + viewport) {
                    this.element.scrollTop = top;
                }
                if (this.nodes.has(target)) this.pendingReveal = null;
            }
            this.element.scrollTop = Math.max(0, Math.min(this.element.scrollTop, this.totalHeight() - viewport));
            const first = this.index.at(this.element.scrollTop);
            const last = this.index.at(this.element.scrollTop + viewport);
            const wanted = new Set();
            for (let index = Math.max(0, first - this.overscan); index <= Math.min(this.items.length - 1, last + this.overscan); index++) wanted.add(index);
            const focused = this.focusedIndex();
            if (focused >= 0) wanted.add(focused);
            for (const index of [...this.nodes.keys()]) {
                if (!wanted.has(index) || (this.dirty.has(index) && index !== focused)) this.remove(index);
            }
            for (const index of [...wanted].sort((a, b) => a - b)) {
                let card = this.nodes.get(index);
                if (!card) {
                    card = this.createCard(this.items[index]);
                    this.nodes.set(index, card);
                    const next = [...this.nodes.keys()].filter(key => key > index).sort((a, b) => a - b)[0];
                    this.canvas.insertBefore(card, this.nodes.get(next) ?? null);
                    this.observer?.observe(card);
                }
                // Use logical neighbors, not mounted DOM siblings: group edges
                // remain correct when scrolling, filtering or preserving focus.
                this.decorateCard(card, this.items[index], index, this.items);
                card.style.transform = `translateY(${this.index.offset(index)}px)`;
                card.setAttribute('role', 'listitem');
                card.setAttribute('aria-posinset', index + 1);
                card.setAttribute('aria-setsize', this.items.length);
            }
        }
    };
})();
