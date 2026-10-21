// Verify the exact font/source release pair before packaging. No network access.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const root = path.resolve(__dirname, '../..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'licenses/sources/liberation-fonts-1.07.4.json'), 'utf8'));
for (const item of [manifest.source, ...manifest.fonts]) {
    const bytes = fs.readFileSync(path.join(root, item.file));
    const actual = crypto.createHash('sha256').update(bytes).digest('hex');
    if (actual !== item.sha256) throw new Error('Font/source release mismatch: ' + item.file);
}
console.log('Verified Liberation ' + manifest.version + ': complete source archive and four official fonts.');
