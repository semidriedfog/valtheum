"""Independent VTA1 / VTV2 / VTI1 / VTN1 restorer using Google Tink, cryptography and argon2-cffi.
No valtheum code, database, or server required. Never logs keys/passwords.
Install: python -m pip install tink cryptography argon2-cffi
Usage: python tools/restore_vta.py vault.key file.vta-or-vtv-or-vti restored-file [--notes]
"""
import argparse
import getpass
import hashlib
import json
import os
from pathlib import Path
import struct
import tempfile
from argon2.low_level import hash_secret_raw, Type
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from cryptography.hazmat.primitives import hashes
from tink import cleartext_keyset_handle, streaming_aead
from tink.proto import aes_gcm_hkdf_streaming_pb2, common_pb2, tink_pb2


def read_exact(stream, size):
    out = bytearray()
    while len(out) < size:
        chunk = stream.read(size - len(out))
        if not chunk:
            raise ValueError('Truncated file')
        out.extend(chunk)
    return bytes(out)


def restore(key_path, source, destination, password, notes_only=False):
    destination = Path(destination)
    if destination.exists():
        raise FileExistsError('Destination already exists')
    with open(key_path, 'rb') as f:
        data = f.read(97)
    if len(data) != 96 or data[:8] != b'VLTKEY01':
        raise ValueError('Unsupported vault.key')
    m, t, p = struct.unpack('<III', data[8:20])
    if (m, t, p) != (65536, 3, 1):
        raise ValueError('Unsupported KDF parameters')
    kek = hash_secret_raw(password.encode('utf-8'), data[20:36], t, m, p, 32, Type.ID, version=19)
    master = AESGCM(kek).decrypt(data[36:48], data[48:], data[:48])
    streaming_aead.register()
    temp = None
    try:
        with open(source, 'rb') as encrypted:
            prefix = read_exact(encrypted, 84)
            video = prefix[:8] == b'VLTARC02'
            standalone = prefix[:8] == b'VLTNOTE1'
            image = prefix[:8] == b'VLTIMG01' or standalone
            notes_only = notes_only or standalone
            if notes_only and not image:
                raise ValueError('Notes require a VTI image')
            if prefix[:8] not in (b'VLTARC01', b'VLTARC02', b'VLTIMG01', b'VLTNOTE1'):
                raise ValueError('Unsupported archive version')
            file_key = AESGCM(master).decrypt(prefix[24:36], prefix[36:84], prefix[:36])
            params = aes_gcm_hkdf_streaming_pb2.AesGcmHkdfStreamingParams(
                ciphertext_segment_size=65536, derived_key_size=32, hkdf_hash_type=common_pb2.SHA256)
            key = aes_gcm_hkdf_streaming_pb2.AesGcmHkdfStreamingKey(version=0, params=params, key_value=file_key)
            keyset = tink_pb2.Keyset(primary_key_id=1, key=[tink_pb2.Keyset.Key(
                key_data=tink_pb2.KeyData(type_url='type.googleapis.com/google.crypto.tink.AesGcmHkdfStreamingKey',
                                         value=key.SerializeToString(), key_material_type=tink_pb2.KeyData.SYMMETRIC),
                status=tink_pb2.ENABLED, key_id=1, output_prefix_type=tink_pb2.RAW)])
            # Keyset exists only in memory; never serialize secret keys to disk.
            primitive = cleartext_keyset_handle.from_keyset(keyset).primitive(streaming_aead.StreamingAead)
            with primitive.new_decrypting_stream(encrypted, prefix) as stream:
                length, = struct.unpack('<I', read_exact(stream, 4))
                if length > (262144 if image else 16384):
                    raise ValueError('Metadata too large')
                metadata = json.loads(read_exact(stream, length))
                formats = ('mp4', 'webm', 'mkv', 'avi', 'mov', 'm4v', 'wmv') if video else ('zip', 'tar.zst')
                if image:
                    formats = ('json',) if standalone else ('jpg','jpeg','png','webp','gif','bmp','avif')
                if metadata['format'] not in formats:
                    raise ValueError('Unsupported original format')
                if not isinstance(metadata['size'], int) or not 0 <= metadata['size'] <= 1024**4:
                    raise ValueError('Invalid plaintext size')
                if video and (metadata['sha256'] != '' or not metadata['name'].lower().endswith('.' + metadata['format'])):
                    raise ValueError('Invalid video metadata')
                fd, temp = tempfile.mkstemp(prefix='.restore-', dir=destination.parent)
                digest = hashlib.sha256()
                count = 0
                with os.fdopen(fd, 'wb') as output:
                    while count < metadata['size']:
                        chunk = read_exact(stream, min(65536, metadata['size'] - count))
                        count += len(chunk)
                        digest.update(chunk)
                        if not notes_only:
                            output.write(chunk)
                    if video:
                        metadata['sha256'] = read_exact(stream, 32).hex()
                    if image:
                        revision = metadata.get('revision')
                        if not isinstance(revision,str) or len(revision)!=32 or any(c not in '0123456789abcdefABCDEF' for c in revision) or not metadata['name'].lower().endswith('.'+metadata['format']):
                            raise ValueError('Invalid image metadata')
                        records = metadata.get('notes', [])
                        if not isinstance(records, list) or len(records) > 128 or (standalone and (len(records)!=1 or metadata['size']!=0)):
                            raise ValueError('Invalid note count')
                        total = metadata.get('notes_size')
                        if not isinstance(total, int) or not 0 <= total <= 8*1024*1024:
                            raise ValueError('Invalid notes size')
                        note_key = HKDF(algorithm=hashes.SHA256(),length=32,salt=prefix[8:24],info=b'VTN1 note records' if standalone else b'VTI1 note records').derive(file_key)
                        restored_notes, ids, nonces, offset = [], set(), set(), 0
                        for record in records:
                            note_id = record['id']
                            if not isinstance(note_id,str) or len(note_id)!=32 or any(c not in '0123456789abcdefABCDEF' for c in note_id) or note_id in ids:
                                raise ValueError('Invalid note ID')
                            ids.add(note_id)
                            if not isinstance(record['title'], str) or len(record['title'].encode('utf-8')) > 512:
                                raise ValueError('Invalid note title')
                            size = record['length']
                            nonce = bytes(record['nonce'])
                            if len(nonce)!=12 or nonce in nonces or record['offset']!=offset or not isinstance(size,int) or not 16<=size<=262160 or offset+size>total:
                                raise ValueError('Invalid note record')
                            nonces.add(nonce)
                            text = AESGCM(note_key).decrypt(nonce,read_exact(stream,size),note_id.encode('ascii')).decode('utf-8')
                            offset += size
                            restored_notes.append({'id':note_id,'title':record['title'],'body':text})
                        if offset != total:
                            raise ValueError('Invalid note tail length')
                        if notes_only:
                            output.write(json.dumps(restored_notes,ensure_ascii=False,indent=2).encode('utf-8'))
                    if stream.read(1):
                        raise ValueError('Unexpected extra plaintext')
                    if count != metadata['size'] or digest.hexdigest() != metadata['sha256']:
                        raise ValueError('Restored file verification failed')
                    output.flush()
                    os.fsync(output.fileno())
        os.link(temp, destination)  # Atomic no-overwrite publication on the same volume.
        return metadata
    finally:
        if temp is not None:
            os.unlink(temp)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('key_file')
    parser.add_argument('encrypted_file')
    parser.add_argument('destination')
    parser.add_argument('--notes', action='store_true', help='Export VTI notes as plaintext JSON instead of the image')
    args = parser.parse_args()
    try:
        meta = restore(args.key_file, args.encrypted_file, args.destination, getpass.getpass('Password: '),args.notes)
    except Exception as exc:
        raise SystemExit(f'Restore failed: {type(exc).__name__}: {exc}') from exc
    if meta["format"] == "json" or args.notes:
        print("Restored note JSON.")
    else:
        print(f'Restored {meta["size"]} bytes. SHA-256: {meta["sha256"]}')


if __name__ == '__main__':
    main()
