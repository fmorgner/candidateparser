//! C FFI bindings for *candidateparser* library.
//!
//! The header file is provided by this library, you can find it in the crate source code under
//! [`src/candidateparser.h`](https://github.com/dbrgn/candidateparser/blob/master/candidateparser-ffi/candidateparser.h).
//!
//! You can find an example C program under
//! [`src/example.c`](https://github.com/dbrgn/candidateparser/blob/master/candidateparser-ffi/example.c).
#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

extern crate candidateparser;
extern crate libc;

use libc::{c_char, size_t, uint8_t};
use std::boxed::Box;
use std::ffi::{CStr, CString};
use std::mem;
use std::ptr;

/// A key value pair.
#[derive(Debug)]
#[repr(C)]
pub struct KeyValuePair {
    pub key: *const uint8_t,
    pub key_len: size_t,
    pub val: *const uint8_t,
    pub val_len: size_t,
}

/// A key value map.
///
/// The `len` must be set to the length of the `values` array. Everything else
/// is undefined behavior!
#[derive(Debug)]
#[repr(C)]
pub struct KeyValueMap {
    pub values: *const KeyValuePair,
    pub len: size_t,
}

/// A wrapper around the `IceCandidate` data that is C compatible.
#[derive(Debug)]
#[repr(C)]
pub struct IceCandidateFFI {
    pub foundation: *const c_char,
    pub component_id: u32,
    pub transport: *const c_char,
    pub priority: u64,
    pub connection_address: *const c_char,
    pub port: u16,
    pub candidate_type: *const c_char,
    /// The address is optional. If no value is defined, this will contain a
    /// null pointer.
    pub rel_addr: *const c_char,
    /// This port is optional. If no address is defined, this will contain the
    /// value `0`.
    pub rel_port: u16,
    /// The extensions map will always be defined but may be empty.
    pub extensions: KeyValueMap,
}

/// Parse an ICE candidate SDP string and return a pointer to an
/// [`IceCandidateFFI`](struct.IceCandidateFFI.html) struct.
///
/// Make sure to always call the [`free_ice_candidate`](fn.free_ice_candidate.html)
/// function after you're done processing the data, to prevent memory leaks!
///
/// This function is marked `unsafe` because it dereferences raw pointers.
#[no_mangle]
pub unsafe extern "C" fn parse_ice_candidate_sdp(sdp: *const c_char) -> *const IceCandidateFFI {
    // Convert C string to Rust byte slice
    if sdp.is_null() {
        return ptr::null();
    }
    let cstr_sdp = CStr::from_ptr(sdp);

    // Parse
    let parsed = match candidateparser::parse(cstr_sdp.to_bytes()) {
        Some(candidate) => candidate,
        None => return ptr::null(),
    };

    // Convert to FFI representation
    let transport_cstring: CString = parsed.transport.into();
    let candidate_type_cstring: CString = parsed.candidate_type.into();
    let extensions = match parsed.extensions {
        Some(e) => {
            // Create KeyValuePairs from map entries
            let mut extensions_vec = e.iter().map(|(k, v)| {
                let mut k_vec = k.clone();
                k_vec.shrink_to_fit();
                assert_eq!(k_vec.len(), k_vec.capacity());
                let mut v_vec = v.clone();
                v_vec.shrink_to_fit();
                assert_eq!(v_vec.len(), v_vec.capacity());
                let pair = KeyValuePair {
                    key: k_vec.as_ptr(),
                    key_len: k_vec.len(),
                    val: v_vec.as_ptr(),
                    val_len: v_vec.len(),
                };
                mem::forget(k_vec);
                mem::forget(v_vec);
                pair
            }).collect::<Vec<KeyValuePair>>();

            // Shrink vector so that capacity == length
            extensions_vec.shrink_to_fit();
            assert_eq!(extensions_vec.len(), extensions_vec.capacity());

            // Create KeyValueMap
            let map = KeyValueMap {
                values: extensions_vec.as_ptr(),
                len: extensions_vec.len(),
            };

            // Prevent temporary vector from being deallocated
            mem::forget(extensions_vec);

            map
        },
        None => KeyValueMap {
            values: ptr::null(),
            len: 0,
        },
    };
    let boxed = Box::new(IceCandidateFFI {
        foundation: CString::new(parsed.foundation).unwrap().into_raw(),
        component_id: parsed.component_id,
        transport: transport_cstring.into_raw(),
        priority: parsed.priority,
        connection_address: CString::new(parsed.connection_address.to_string()).unwrap().into_raw(),
        port: parsed.port,
        candidate_type: candidate_type_cstring.into_raw(),
        rel_addr: match parsed.rel_addr {
            Some(addr) => CString::new(addr.to_string()).unwrap().into_raw(),
            None => ptr::null(),
        },
        rel_port: parsed.rel_port.unwrap_or(0),
        extensions: extensions,
    });

    Box::into_raw(boxed)
}

/// Free the memory associated with the [`IceCandidateFFI`](struct.IceCandidateFFI.html) struct.
///
/// Make sure to always call this function after you're done processing the
/// data, otherwise you'll end up with memory leaks!
///
/// This function is marked `unsafe` because it dereferences raw pointers.
#[no_mangle]
pub unsafe extern "C" fn free_ice_candidate(ptr: *const IceCandidateFFI) {
    if ptr.is_null() { return; }
    let ptr = ptr as *mut IceCandidateFFI;
    let candidate: Box<IceCandidateFFI> = Box::from_raw(ptr);
    CString::from_raw(candidate.foundation as *mut c_char);
    CString::from_raw(candidate.transport as *mut c_char);
    CString::from_raw(candidate.connection_address as *mut c_char);
    CString::from_raw(candidate.candidate_type as *mut c_char);
    if !candidate.rel_addr.is_null() {
        CString::from_raw(candidate.rel_addr as *mut c_char);
    }
    let e = candidate.extensions;
    let pairs = Vec::from_raw_parts(e.values as *mut KeyValuePair, e.len as usize, e.len as usize);
    for p in pairs {
        Vec::from_raw_parts(p.key as *mut uint8_t, p.key_len as usize, p.key_len as usize);
        Vec::from_raw_parts(p.val as *mut uint8_t, p.val_len as usize, p.val_len as usize);
    }
    // Resources will be freed here
}


#[cfg(test)]
mod tests {

    use std::ffi::CString;

    use super::*;

    #[test]
    fn test_parse_ice_candidate_sdp() {
        // Same data as test_parse_full in the `candidateparser` crate.
        let sdp = CString::new("candidate:842163049 1 udp 1686052607 1.2.3.4 46154 typ srflx raddr 10.0.0.17 rport 46154 generation 0 ufrag EEtu network-id 3 network-cost 10").unwrap();

        // Parse
        let parsed: *const IceCandidateFFI = unsafe { parse_ice_candidate_sdp(sdp.into_raw()) };

        // Restore
        let candidate: Box<IceCandidateFFI> = unsafe { Box::from_raw(parsed as *mut IceCandidateFFI) };

        let foundation = unsafe { CString::from_raw(candidate.foundation as *mut c_char) };
        let transport = unsafe { CString::from_raw(candidate.transport as *mut c_char) };
        let connection_address = unsafe { CString::from_raw(candidate.connection_address as *mut c_char) };
        let candidate_type = unsafe { CString::from_raw(candidate.candidate_type as *mut c_char) };
        let rel_addr = unsafe { CString::from_raw(candidate.rel_addr as *mut c_char) };
        assert_eq!(foundation, CString::new("842163049").unwrap());
        assert_eq!(candidate.component_id, 1);
        assert_eq!(transport, CString::new("udp").unwrap());
        assert_eq!(candidate.priority, 1686052607);
        assert_eq!(connection_address, CString::new("1.2.3.4").unwrap());
        assert_eq!(candidate.port, 46154);
        assert_eq!(candidate_type, CString::new("srflx").unwrap());
        assert_eq!(rel_addr, CString::new("10.0.0.17").unwrap());
        assert_eq!(candidate.rel_port, 46154);
        assert_eq!(candidate.extensions.len, 4);
    }
}
