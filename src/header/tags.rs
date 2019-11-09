use num_derive::{FromPrimitive, ToPrimitive};
use strum_macros::Display;

#[derive(Debug, Copy, Clone, FromPrimitive, ToPrimitive, Display, PartialEq, Eq, Hash)]
pub enum Tag {
    Image = 61,
    Signatures = 62,
    Immutable = 63,
    Regions = 64,
    I18nTable = 100,
    Sigbase = 256,
    Sigsize = 257,
    Siglemd5_1 = 258,
    Sigpgp = 259,
    Siglemd5_2 = 260,
    Sigmd5 = 261,
    Siggpg = 262,
    Sigpgp5 = 263,
    Badsha1_1 = 264,
    Badsha1_2 = 265,
    Pubkeys = 266,
    Dsaheader = 267,
    Rsaheader = 268,
    Sha1header = 269,
    Longsigsize = 270,
    Longarchivesize = 271,
    Name = 1000,
    Version = 1001,
    Release = 1002,
    Epoch = 1003,
    Summary = 1004,
    Description = 1005,
    BuildTime = 1006,
    BuildHost = 1007,
    InstallTime = 1008,
    Size = 1009,
    Distribution = 1010,
    Vendor = 1011,
    Gif = 1012,
    Xpm = 1013,
    License = 1014,
    Packager = 1015,
    Group = 1016,
    Changelog = 1017,
    Source = 1018,
    Patch = 1019,
    Url = 1020,
    Os = 1021,
    Arch = 1022,
    PreIn = 1023,
    PostIn = 1024,
    ReUn = 1025,
    PostUn = 1026,
    OldFileNames = 1027,
    FileSizes = 1028,
    FilesStates = 1029,
    FileModes = 1030,
    FileUIDs = 1031,
    FileGids = 1032,
    FilerDevs = 1033,
    FileMTimes = 1034,
    FileMD5s = 1035,
    FileLinktos = 1036,
    FileFlags = 1037,
    Root = 1038,
    FileUserName = 1039,
    FileGroupName = 1040,
    Exclude = 1041,
    Exclusive = 1042,
    Icon = 1043,
    SourceRpm = 1044,
    FileVerifyFlags = 1045,
    ArchiveSize = 1046,
    ProvideName = 1047,
    RequireFlags = 1048,
    RequireName = 1049,
    RequireVersion = 1050,
    NoSource = 1051,
    NoPatch = 1052,
    Conflictflags = 1053,
    Conflictname = 1054,
    Conflictversion = 1055,
    DefaultPrefix = 1056,
    BuildRoot = 1057,
    Installprefix = 1058,
    Excludearch = 1059,
    Excludeos = 1060,
    Exclusivearch = 1061,
    Exclusiveos = 1062,
    Autoreqprov = 1063,
    Rpmversion = 1064,
    Triggerscripts = 1065,
    Triggername = 1066,
    Triggerversion = 1067,
    Triggerflags = 1068,
    Triggerindex = 1069,
    Verifyscript = 1079,
    Changelogtime = 1080,
    ChangelogName = 1081,
    Changelogtext = 1082,
    Brokenmd5 = 1083,
    Prereq = 1084,
    Preinprog = 1085,
    Postinprog = 1086,
    Preunprog = 1087,
    Postunprog = 1088,
    BuildArchs = 1089,
    Obsoletename = 1090,
    Verifyscriptprog = 1091,
    Triggerscriptprog = 1092,
    DocDir = 1093,
    Cookie = 1094,
    FileDevices = 1095,
    FileInodes = 1096,
    FileLangs = 1097,
    Prefixes = 1098,
    InstPrefixes = 1099,
    Triggerin = 1100,
    Triggerun = 1101,
    Triggerpostun = 1102,
    Autoreq = 1103,
    Autoprov = 1104,
    Capability = 1105,
    Sourcepackage = 1106,
    Oldorigfilenames = 1107,
    Buildprereq = 1108,
    Buildrequires = 1109,
    Buildconflicts = 1110,
    Buildmacros = 1111,
    Provideflags = 1112,
    Provideversion = 1113,
    Obsoleteflags = 1114,
    Obsoleteversion = 1115,
    Dirindexes = 1116,
    Basenames = 1117,
    DirNames = 1118,
    Origdirindexes = 1119,
    Origbasenames = 1120,
    Origdirnames = 1121,
    Optflags = 1122,
    Disturl = 1123,
    Payloadformat = 1124,
    Payloadcompressor = 1125,
    Payloadflags = 1126,
    InstallColor = 1127,
    InstallTid = 1128,
    Removetid = 1129,
    Sha1rhn = 1130,
    RHNPlatform = 1131,
    Platform = 1132,
    PatchesName = 1133,
    Catchesflags = 1134,
    Catchesversion = 1135,
    Cachectime = 1136,
    Cachepkgpath = 1137,
    Cachepkgsize = 1138,
    Cachepkgmtime = 1139,
    Filecolors = 1140,
    Fileclass = 1141,
    Classdict = 1142,
    Diledependsx = 1143,
    Filedependsn = 1144,
    Dependsdict = 1145,
    Sourcepkgid = 1146,
    FileContexts = 1147,
    FsContects = 1148,
    ReContexts = 1149,
    Policies = 1150,
    Posttrans = 1152,
    Pretransprog = 1153,
    Posttransprog = 1154,
    Disttag = 1155,
    // remove absolete and unimplemented
    Triggerprein = 1171,
    Dbinstance = 1195,
    // tags 1997-4999 reserved
    Filenames = 5000,
    Fileprovide = 5001,
    Filerequire = 5002,
    Fsnames = 5003,
    Fssizes = 5004,
    Triggerconds = 5005,
    Triggertype = 5006,
    Origfilenames = 5007,
    Longfilesizes = 5008,
    Longsize = 5009,
    Filecaps = 5010,
    Filedigestalgo = 5011,
    Bugurl = 5012,
    Evr = 5013,
    Nvr = 5014,
    Nevr = 5015,
    Nevra = 5016,
    Headercolor = 5017,
    Verbose = 5018,
    Epochnum = 5019,
    Preinflags = 5020,
    Postinflags = 5021,
    Preunflags = 5022,
    Postunflags = 5023,
    Pretransflags = 5024,
    Posttransflags = 5025,
    Verifyscriptflags = 5026,
    Triggerscriptflags = 5027,
    Collections = 5029,
    Policynames = 5030,
    Policytypes = 5031,
    Policytypesindexes = 5032,
    Policyflags = 5033,
    Vcs = 5034,
    Ordername = 5035,
    Orderversion = 5036,
    Orderflags = 5037,
    Mssfmanifest = 5038,
    Mssfdomain = 5039,
    Instfilenames = 5040,
    Requirenevrs = 5041,
    Providenevrs = 5042,
    Obsoletenevrs = 5043,
    Conflictnevrs = 5044,
    Filenlinks = 5045,
    Recommendname = 5046,
    Recommendversion = 5047,
    Recommendflags = 5048,
    Suggestname = 5049,
    Suggestversion = 5050,
    Suggestflags = 5051,
    Supplementname = 5052,
    Supplementversion = 5053,
    Supplementflags = 5054,
    Enhancename = 5055,
    Enhanceversion = 5056,
    Enhanceflags = 5057,
    Recommendnevrs = 5058,
    Suggestnevrs = 5059,
    Supplementnevrs = 5060,
    Enhancenevrs = 5061,
    Encoding = 5062,
    Filetriggerin = 5063,
    Filetriggerun = 5064,
    Filetriggerpostun = 5065,
    Filetriggerscripts = 5066,
    Filetriggerscriptprog = 5067,
    Filetriggerscriptflags = 5068,
    Filetriggername = 5069,
    Filetriggerindex = 5070,
    Filetriggerversion = 5071,
    Filetriggerflags = 5072,
    Transfiletriggerin = 5073,
    Transfiletriggerun = 5074,
    Transfiletriggerpostun = 5075,
    Transfiletriggerscripts = 5076,
    Transfiletriggerscriptprog = 5077,
    Transfiletriggerscriptflags = 5078,
    Transfiletriggername = 5079,
    Transfiletriggerindex = 5080,
    Transfiletriggerversion = 5081,
    Transfiletriggerflags = 5082,
    Removepathpostfixes = 5083,
    Filetriggerpriorities = 5084,
    Transfiletriggerpriorities = 5085,
    Filetriggerconds = 5086,
    Filetriggertype = 5087,
    Transfiletriggerconds = 5088,
    Transfiletriggertype = 5089,
    Filesignatures = 5090,
    Filesignaturelength = 5091,
    Payloaddigest = 5092,
    Payloaddigestalgo = 5093,
    Autoinstalled = 5094,
    Identity = 5095,
    Modularitylabel = 5096,
    Other = 8888,
}

impl Default for Tag {
    fn default() -> Tag {
        Tag::Other
    }
}

#[derive(Debug, Copy, Clone, FromPrimitive, ToPrimitive, Display, PartialEq, Eq, Hash)]
pub enum SignatureTag {
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

impl Default for SignatureTag {
    fn default() -> SignatureTag {
        SignatureTag::Other
    }
}
