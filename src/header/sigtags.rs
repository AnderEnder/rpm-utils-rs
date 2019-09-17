use num_derive::{FromPrimitive, ToPrimitive};

#[derive(Debug, Copy, Clone, FromPrimitive, ToPrimitive)]
pub enum SigTag {
    HeaderSignatures = 62,
    HeaderImmutable = 63,
    Headeri18Ntable = 100,
    BadSHA1_1 = 264,
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
