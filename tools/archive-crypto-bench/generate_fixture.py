from pathlib import Path
from PIL import Image
import random, io, zipfile, tarfile
root = Path('target/archive-crypto-bench-data')
root.mkdir(parents=True, exist_ok=True)
rng = random.Random(20260927)
with zipfile.ZipFile(root / 'pages.zip', 'w', compression=zipfile.ZIP_STORED) as z, tarfile.open(root / 'pages.tar', 'w') as t:
    for i in range(96):
        img = Image.frombytes('RGB', (1000, 1400), rng.randbytes(1000*1400*3))
        out = io.BytesIO()
        img.save(out, format='JPEG', quality=85)
        data = out.getvalue()
        name = f'{i:04}.jpg'
        z.writestr(name, data)
        info = tarfile.TarInfo(name)
        info.size = len(data)
        t.addfile(info, io.BytesIO(data))
print('Generated 96 synthetic 1000x1400 JPEG pages; ZIP bytes:', (root / 'pages.zip').stat().st_size)
