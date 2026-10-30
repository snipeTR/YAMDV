// SPDX-License-Identifier: AGPL-3.0-or-later
//! Compile-time SHA-256 (FIPS 180-4) for the interactive export's CSP hash (S-EXP-02): the
//! digest of the exact script bytes is a constant computed by the compiler (zero run-time cost).

// Const evaluation: an out-of-range index is a compile error, never a run-time panic.
#![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// Byte `i` of the padded message (`data` ‖ 0x80 ‖ zeros ‖ 64-bit bit length).
const fn padded(data: &[u8], total: usize, i: usize) -> u8 {
    let n = data.len();
    if i < n {
        data[i]
    } else if i == n {
        0x80
    } else if i >= total - 8 {
        let bits = (n as u64).wrapping_mul(8);
        (bits >> (8 * (total - 1 - i))) as u8
    } else {
        0
    }
}

/// SHA-256 digest of `data`, usable in constants.
pub const fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let total = (data.len() + 9).div_ceil(64) * 64;
    let mut off = 0;
    while off < total {
        let mut w = [0u32; 64];
        let mut t = 0;
        while t < 16 {
            let b = off + 4 * t;
            w[t] = u32::from_be_bytes([
                padded(data, total, b),
                padded(data, total, b + 1),
                padded(data, total, b + 2),
                padded(data, total, b + 3),
            ]);
            t += 1;
        }
        while t < 64 {
            let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
            let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
            w[t] = w[t - 16]
                .wrapping_add(s0)
                .wrapping_add(w[t - 7])
                .wrapping_add(s1);
            t += 1;
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        t = 0;
        while t < 64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[t])
                .wrapping_add(w[t]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
            t += 1;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
        off += 64;
    }
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 8 {
        let b = h[i].to_be_bytes();
        out[4 * i] = b[0];
        out[4 * i + 1] = b[1];
        out[4 * i + 2] = b[2];
        out[4 * i + 3] = b[3];
        i += 1;
    }
    out
}

/// Standard base64 (with padding) of a 32-byte digest: 44 ASCII bytes.
pub const fn base64_32(d: &[u8; 32]) -> [u8; 44] {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = [b'='; 44];
    let mut i = 0;
    let mut o = 0;
    while i + 3 <= 30 {
        let n = (d[i] as u32) << 16 | (d[i + 1] as u32) << 8 | d[i + 2] as u32;
        out[o] = T[(n >> 18) as usize & 63];
        out[o + 1] = T[(n >> 12) as usize & 63];
        out[o + 2] = T[(n >> 6) as usize & 63];
        out[o + 3] = T[n as usize & 63];
        i += 3;
        o += 4;
    }
    // Last two bytes (30, 31) → three characters and one '='.
    let n = (d[30] as u32) << 16 | (d[31] as u32) << 8;
    out[40] = T[(n >> 18) as usize & 63];
    out[41] = T[(n >> 12) as usize & 63];
    out[42] = T[(n >> 6) as usize & 63];
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        let abc = sha256(b"abc");
        assert_eq!(
            abc[..4],
            [0xba, 0x78, 0x16, 0xbf],
            "FIPS 180-4 example digest prefix"
        );
        let e = base64_32(&sha256(b""));
        assert_eq!(&e, b"47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=");
    }
}
