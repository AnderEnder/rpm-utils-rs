# Error Handling Migration Guide

This document provides a comprehensive guide for migrating error handling patterns in the rpm-utils-rs project to modern Rust best practices.

---

## Table of Contents

1. [io::Error Pattern Migration](#1-ioerror-pattern-migration)
2. [Library Code Using println!](#2-library-code-using-println)
3. [Excessive use of expect() and unwrap()](#3-excessive-use-of-expect-and-unwrap)
4. [Error Handling in Iterators](#4-error-handling-in-iterators)
5. [Custom Error Types (Future Enhancement)](#5-custom-error-types-future-enhancement)

---

## 1. io::Error Pattern Migration

### Current Pattern (Deprecated)
```rust
io::Error::new(io::ErrorKind::Other, "message")
```

### New Pattern (Rust 1.80+)
```rust
io::Error::other("message")
```

### Why This Matters
- **Cleaner code**: Less verbose, easier to read
- **Future-proof**: Follows modern Rust conventions
- **Maintained**: `io::Error::other()` is the recommended approach

### Migration Steps

**Already covered in detail in CLIPPY_FIXES.md** - See Category 1 for all 24 instances.

---

## 2. Library Code Using println!

### The Problem

**File:** `src/header/index.rs:247-248`
```rust
let tag = T::from_u32(tag_id).unwrap_or_else(|| {
    println!("Unknown tag {}", tag_id);  // ❌ BAD: Library printing to stdout
    T::default()
});
```

**File:** `src/header/index.rs:252-253`
```rust
let itype = Type::from_u32(type_id).unwrap_or_else(|| {
    println!("Unknown type {}", type_id);  // ❌ BAD: Library printing to stdout
    Type::Null
});
```

### Why This Is Wrong

1. **Libraries should not print**: Output control belongs to the application, not the library
2. **Cannot be suppressed**: Users can't disable these messages
3. **Breaks library/application boundary**: Library code should be silent
4. **Testing problems**: These prints appear in test output

### Solution Options

#### Option A: Use the `log` crate (Recommended)

**Add dependency:**
```toml
# Cargo.toml
[dependencies]
log = "0.4"
```

**Update code:**
```rust
use log::warn;

let tag = T::from_u32(tag_id).unwrap_or_else(|| {
    warn!("Unknown tag {}", tag_id);
    T::default()
});

let itype = Type::from_u32(type_id).unwrap_or_else(|| {
    warn!("Unknown type {}", type_id);
    Type::Null
});
```

**Benefits:**
- Users can configure logging level
- Can be disabled in production
- Standard Rust logging pattern
- Application controls output destination

#### Option B: Return Errors

```rust
pub fn read_index<T: Tag, R: Read>(
    reader: &mut R,
    index_size: usize,
) -> io::Result<Vec<Index<T>>> {
    // ...
    let tag = T::from_u32(tag_id)
        .ok_or_else(|| io::Error::other(format!("Unknown tag {}", tag_id)))?;

    let itype = Type::from_u32(type_id)
        .ok_or_else(|| io::Error::other(format!("Unknown type {}", type_id)))?;
    // ...
}
```

**Benefits:**
- Errors are explicit and must be handled
- No silent fallback behavior
- More robust parsing

#### Option C: Silent Fallback (Current, but remove println!)

```rust
let tag = T::from_u32(tag_id).unwrap_or_else(|| T::default());
let itype = Type::from_u32(type_id).unwrap_or_else(|| Type::Null);
```

**Benefits:**
- Simplest fix
- Maintains current behavior
- No new dependencies

**Drawbacks:**
- Silent failures may hide bugs
- Unknown tags are lost

### Recommendation

**Use Option A (log crate)** for production code. This provides the best balance of:
- Silent library code
- User control
- Debugging capability

---

## 3. Excessive use of expect() and unwrap()

### The Problem

These methods panic on errors, which is unacceptable for library code. Libraries should return `Result` or `Option` and let the application decide how to handle failures.

### Current Issues

#### Issue 3.1: src/header/mod.rs:53-58

```rust
pub fn get_as_string(&self, name: T) -> String {
    self.get_value(name)
        .expect("Tag: not found")           // ❌ Panics if tag missing
        .as_string()
        .expect("Tag: is not a string")     // ❌ Panics if wrong type
}
```

**Problem:** Panics are unrecoverable crashes. Library code should never panic on user input.

**Solution:**
```rust
pub fn get_as_string(&self, name: T) -> io::Result<String> {
    self.get_value(name)
        .ok_or_else(|| io::Error::other(format!("Tag not found: {:?}", name)))?
        .as_string()
        .ok_or_else(|| io::Error::other("Tag is not a string"))
}
```

**Or return Option:**
```rust
pub fn get_as_string(&self, name: T) -> Option<String> {
    self.get_value(name)?.as_string()
}
```

#### Issue 3.2: src/rpm/info.rs:173

```rust
.expect("create a name with 66 bytes");
```

**Context needed** - But general solution:
```rust
// If this truly cannot fail
.expect("BUG: failed to create 66-byte name - this should never happen");

// Or better, handle the error:
.map_err(|e| io::Error::other(format!("Failed to create name: {}", e)))?;
```

#### Issue 3.3: src/payload/cpio.rs:354, 377 (in iterators)

```rust
impl<T: Read + Seek> Iterator for CpioFiles<T> {
    type Item = (FileEntry, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut bytes = Vec::new();
        let (entry, _) = read_entry(&mut self.reader, &mut bytes).unwrap();  // ❌ Panics on I/O error
        if entry.name != TRAILER {
            Some((entry, bytes))
        } else {
            None
        }
    }
}
```

**Problem:** I/O errors cause panics instead of graceful error handling.

**Solution:** Use fallible iterator pattern:
```rust
impl<T: Read + Seek> Iterator for CpioFiles<T> {
    type Item = io::Result<(FileEntry, Vec<u8>)>;  // ← Return Result

    fn next(&mut self) -> Option<Self::Item> {
        let mut bytes = Vec::new();
        match read_entry(&mut self.reader, &mut bytes) {
            Ok((entry, size)) => {
                if entry.name != TRAILER {
                    Some(Ok((entry, bytes)))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),  // ← Return error instead of panic
        }
    }
}
```

**Usage:**
```rust
for result in cpio_files {
    match result {
        Ok((entry, data)) => {
            // Process entry
        }
        Err(e) => {
            eprintln!("Error reading entry: {}", e);
            break;
        }
    }
}
```

**Note:** These structs are currently dead code and should be removed, but this is the proper pattern if they're revived.

#### Issue 3.4: src/header/mod.rs:260

```rust
.expect("timestamp conversion failed")
```

**Solution:**
```rust
.map_err(|_| io::Error::other("timestamp conversion failed"))?
```

### Migration Checklist

- [ ] Replace all `expect()` in public APIs with `Result` returns
- [ ] Replace all `unwrap()` in library code with proper error handling
- [ ] Allow `expect()` only for:
  - Test code
  - Truly unreachable cases with detailed messages
  - Internal invariants with "BUG:" prefix
- [ ] Update function signatures to return `Result` or `Option`
- [ ] Update documentation to reflect new error handling

### Best Practices

#### When to use expect() (rare cases)

```rust
// ✅ OK: Internal invariant that should never fail
let value = calculation()
    .expect("BUG: impossible state reached - please file an issue");

// ✅ OK: Test code
#[test]
fn test_something() {
    let data = parse_data().expect("test data should be valid");
}
```

#### When to use unwrap() (never in library code)

```rust
// ❌ NEVER in library code
// ✅ Only in:
// - Examples
// - Test code
// - Application code (not library code)
// - After checking with is_some()/is_ok()
```

---

## 4. Error Handling in Iterators

### Pattern: Fallible Iterators

When iterators can fail (I/O errors, parsing errors), return `Result`:

```rust
// ❌ BAD: Panics on error
impl Iterator for MyIterator {
    type Item = Data;
    fn next(&mut self) -> Option<Data> {
        Some(self.read().unwrap())  // ← Panics!
    }
}

// ✅ GOOD: Returns errors
impl Iterator for MyIterator {
    type Item = io::Result<Data>;
    fn next(&mut self) -> Option<io::Result<Data>> {
        Some(self.read())  // ← Returns Result
    }
}
```

### Alternative: Collect results early

```rust
// Instead of lazy fallible iterator
let items: Result<Vec<Data>, Error> = iterator
    .map(|x| x.parse())
    .collect();  // ← Collects all or returns first error
```

---

## 5. Custom Error Types (Future Enhancement)

### Current State
All errors use `io::Error`, which is generic.

### Recommendation for Future
Consider a custom error type for better error handling:

```rust
use std::fmt;

#[derive(Debug)]
pub enum RpmError {
    Io(io::Error),
    InvalidMagic { expected: Vec<u8>, found: Vec<u8> },
    UnsupportedVersion { major: u8, minor: u8 },
    InvalidTag(u32),
    InvalidType(u32),
    CompressionError(String),
    PathTraversal(String),
}

impl fmt::Display for RpmError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RpmError::Io(e) => write!(f, "I/O error: {}", e),
            RpmError::InvalidMagic { expected, found } => {
                write!(f, "Invalid magic bytes: expected {:?}, found {:?}", expected, found)
            }
            RpmError::UnsupportedVersion { major, minor } => {
                write!(f, "Unsupported RPM version: {}.{}", major, minor)
            }
            // ... other variants
        }
    }
}

impl std::error::Error for RpmError {}

impl From<io::Error> for RpmError {
    fn from(e: io::Error) -> Self {
        RpmError::Io(e)
    }
}
```

**Benefits:**
- Better error messages
- Type-safe error handling
- Easier debugging
- Can match on specific errors

**Example usage:**
```rust
pub fn read<R: Read>(reader: &mut R) -> Result<Lead, RpmError> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;  // ← io::Error auto-converts to RpmError

    if magic != MAGIC {
        return Err(RpmError::InvalidMagic {
            expected: MAGIC.to_vec(),
            found: magic.to_vec(),
        });
    }
    // ...
}
```

**Note:** This is a future enhancement. Current focus is on fixing immediate issues.

---

## Migration Priority

### High Priority (Do Now)
1. ✅ Remove `println!` from library code → Use `log` crate
2. ✅ Fix `io::Error::new` → Use `io::Error::other` (see CLIPPY_FIXES.md)
3. ✅ Replace `expect()` in public APIs → Return `Result`

### Medium Priority (Soon)
4. Replace `unwrap()` in iterators → Use fallible iterator pattern
5. Add error handling tests
6. Document error conditions in public APIs

### Low Priority (Future)
7. Consider custom error type
8. Add error context with better messages
9. Standardize error message format

---

## Testing Error Handling

Add tests for error cases:

```rust
#[test]
fn test_invalid_magic_returns_error() {
    let mut data = vec![0, 0, 0, 0]; // Wrong magic
    let result = Lead::read(&mut data.as_slice());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not rpm"));
}

#[test]
fn test_missing_tag_returns_error() {
    let header = Header::new();
    let result = header.get_as_string(Tag::Name);
    assert!(result.is_err());  // After fixing to return Result
}
```

---

## Summary

### Key Principles

1. **Libraries should be silent**: No `println!` or `eprintln!`
2. **Libraries should not panic**: No `expect()` or `unwrap()` on user input
3. **Errors should be explicit**: Return `Result` or `Option`
4. **Users should control output**: Use `log` crate for diagnostics

### Quick Reference

| Current Pattern | New Pattern | When |
|----------------|-------------|------|
| `println!(...)` | `log::warn!(...)` or remove | Always |
| `io::Error::new(ErrorKind::Other, msg)` | `io::Error::other(msg)` | Always |
| `.expect("msg")` on public API | `?` or `.map_err()` | Always |
| `.unwrap()` in library | `?` or `.ok_or()?` | Always |
| Panicking iterator | `type Item = Result<T>` | I/O operations |

---

**Estimated Migration Time:** 2-3 hours for all error handling improvements.
