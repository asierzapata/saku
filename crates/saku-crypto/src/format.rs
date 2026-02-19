use crate::error::CryptoError;

/// Magic bytes identifying a saku-crypto file.
pub(crate) const MAGIC: &[u8; 4] = b"SAKU";

/// Current file format version.
pub(crate) const VERSION_V1: u8 = 1;

// Header layout (117 bytes total):
//   [0..4)    magic          4 bytes
//   [4]       version        1 byte
//   [5..21)   kek_salt      16 bytes
//   [21..45)  dek_nonce     24 bytes
//   [45..93)  enc_dek       48 bytes (32 key + 16 tag)
//   [93..117) file_nonce    24 bytes
pub(crate) const HEADER_LEN: usize = 117;

const MAGIC_END: usize = 4;
const VERSION_OFF: usize = 4;
const SALT_OFF: usize = 5;
const SALT_END: usize = 21;
const DEK_NONCE_OFF: usize = 21;
const DEK_NONCE_END: usize = 45;
const ENC_DEK_OFF: usize = 45;
const ENC_DEK_END: usize = 93;
const FILE_NONCE_OFF: usize = 93;
const FILE_NONCE_END: usize = 117;

/// Parsed file header.
pub(crate) struct FileHeader {
    pub kek_salt: [u8; 16],
    pub dek_nonce: [u8; 24],
    pub enc_dek: [u8; 48],
    pub file_nonce: [u8; 24],
}

/// Assemble a complete encrypted file blob from header fields and ciphertext.
pub(crate) fn write_file(header: &FileHeader, ciphertext: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION_V1);
    out.extend_from_slice(&header.kek_salt);
    out.extend_from_slice(&header.dek_nonce);
    out.extend_from_slice(&header.enc_dek);
    out.extend_from_slice(&header.file_nonce);
    out.extend_from_slice(ciphertext);
    out
}

/// Parse an encrypted file blob into header + ciphertext.
pub(crate) fn read_file(data: &[u8]) -> Result<(FileHeader, &[u8]), CryptoError> {
    if data.len() < HEADER_LEN {
        return Err(CryptoError::HeaderTooShort);
    }

    if &data[..MAGIC_END] != MAGIC {
        return Err(CryptoError::InvalidMagic);
    }

    let version = data[VERSION_OFF];
    if version != VERSION_V1 {
        return Err(CryptoError::UnsupportedVersion(version));
    }

    let mut kek_salt = [0u8; 16];
    kek_salt.copy_from_slice(&data[SALT_OFF..SALT_END]);

    let mut dek_nonce = [0u8; 24];
    dek_nonce.copy_from_slice(&data[DEK_NONCE_OFF..DEK_NONCE_END]);

    let mut enc_dek = [0u8; 48];
    enc_dek.copy_from_slice(&data[ENC_DEK_OFF..ENC_DEK_END]);

    let mut file_nonce = [0u8; 24];
    file_nonce.copy_from_slice(&data[FILE_NONCE_OFF..FILE_NONCE_END]);

    let header = FileHeader {
        kek_salt,
        dek_nonce,
        enc_dek,
        file_nonce,
    };

    Ok((header, &data[HEADER_LEN..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        let header = FileHeader {
            kek_salt: [1u8; 16],
            dek_nonce: [2u8; 24],
            enc_dek: [3u8; 48],
            file_nonce: [4u8; 24],
        };
        let ciphertext = b"hello encrypted world";

        let blob = write_file(&header, ciphertext);
        assert_eq!(blob.len(), HEADER_LEN + ciphertext.len());

        let (parsed, ct) = read_file(&blob).unwrap();
        assert_eq!(parsed.kek_salt, [1u8; 16]);
        assert_eq!(parsed.dek_nonce, [2u8; 24]);
        assert_eq!(parsed.enc_dek, [3u8; 48]);
        assert_eq!(parsed.file_nonce, [4u8; 24]);
        assert_eq!(ct, ciphertext);
    }

    #[test]
    fn rejects_short_input() {
        let data = [0u8; 50];
        assert!(matches!(read_file(&data), Err(CryptoError::HeaderTooShort)));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut data = [0u8; HEADER_LEN];
        data[..4].copy_from_slice(b"NOPE");
        data[4] = VERSION_V1;
        assert!(matches!(read_file(&data), Err(CryptoError::InvalidMagic)));
    }

    #[test]
    fn rejects_bad_version() {
        let mut data = [0u8; HEADER_LEN];
        data[..4].copy_from_slice(MAGIC);
        data[4] = 99;
        assert!(matches!(
            read_file(&data),
            Err(CryptoError::UnsupportedVersion(99))
        ));
    }
}
