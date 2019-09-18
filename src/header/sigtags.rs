use num_derive::{FromPrimitive, ToPrimitive};

#[derive(Debug, Copy, Clone, FromPrimitive, ToPrimitive)]
pub enum SigTag {
    HeaderImage = 61,
    HeaderSignatures = 62,
    HeaderImmutable = 63,
    HeaderRegions = 64,
    Headeri18Ntable = 100,
    SigSize = 257,
    SigLEMD5_1 = 258,
    SigPGP = 259,
    SigLEMD5_2 = 260,
    SigMD5 = 261,
    SigGPG = 262,
    SigGPG5 = 263,
    BadSHA1_1 = 264,
    BadSHA1_2 = 265,
    PubKeys = 266,
    DSAHeader = 267,
    RSAHeader = 268,
    SHA1Header = 269,
    LongSigSize = 270,
    LongArchiveSize = 271,
    SHA256Header = 273,
    Size = 1000,
    LEMD5_1 = 1001,
    PGP = 1002,
    LEMD5_2 = 1003,
    MD5 = 1004,
    GPG = 1005,
    PGP5 = 1006,
    PayloadSize = 1007,
    ReservedSpace = 1008,
    Other,
}

impl Default for SigTag {
    fn default() -> SigTag {
        SigTag::Other
    }
}
