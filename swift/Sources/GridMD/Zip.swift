// Minimal deterministic ZIP writer (STORE) + reader (STORE / DEFLATE).
// XLSX is a ZIP of XML parts; a STORE-only package is valid and byte-stable.
// Port of js/src/xlsx/zip.js. DEFLATE-compressed entries (produced by other
// tools) are inflated with Apple's `Compression` framework (raw DEFLATE ==
// COMPRESSION_ZLIB); our own output is STORE, so writer round-trips need no
// inflate path.

import Foundation
import Compression

private let crcTable: [UInt32] = {
    var t = [UInt32](repeating: 0, count: 256)
    for n in 0..<256 {
        var c = UInt32(n)
        for _ in 0..<8 {
            c = (c & 1) != 0 ? 0xEDB8_8320 ^ (c >> 1) : c >> 1
        }
        t[n] = c
    }
    return t
}()

func crc32(_ buf: [UInt8]) -> UInt32 {
    var c: UInt32 = 0xFFFF_FFFF
    for b in buf {
        c = crcTable[Int((c ^ UInt32(b)) & 0xFF)] ^ (c >> 8)
    }
    return c ^ 0xFFFF_FFFF
}

struct ZipEntry {
    let name: String
    let data: [UInt8]
}

enum Zip {
    private static func u16(_ v: Int) -> [UInt8] { [UInt8(v & 0xFF), UInt8((v >> 8) & 0xFF)] }
    private static func u32(_ v: UInt32) -> [UInt8] {
        [UInt8(v & 0xFF), UInt8((v >> 8) & 0xFF), UInt8((v >> 16) & 0xFF), UInt8((v >> 24) & 0xFF)]
    }

    /// Writes entries in order with a fixed DOS timestamp (1980-01-01) so output
    /// is deterministic.
    static func write(_ entries: [ZipEntry]) -> [UInt8] {
        var locals: [UInt8] = []
        var centrals: [UInt8] = []
        var offset = 0

        for entry in entries {
            let nameBuf = Array(entry.name.utf8)
            let body = entry.data
            let crc = crc32(body)

            var local: [UInt8] = []
            local += u32(0x0403_4B50)
            local += u16(20)   // version needed
            local += u16(0)    // flags
            local += u16(0)    // method: STORE
            local += u16(0)    // dos time
            local += u16(0x21) // dos date: 1980-01-01
            local += u32(crc)
            local += u32(UInt32(body.count))
            local += u32(UInt32(body.count))
            local += u16(nameBuf.count)
            local += u16(0)    // extra len
            locals += local
            locals += nameBuf
            locals += body

            var central: [UInt8] = []
            central += u32(0x0201_4B50)
            central += u16(20) // version made by
            central += u16(20) // version needed
            central += u16(0)  // flags
            central += u16(0)  // method
            central += u16(0)  // time
            central += u16(0x21) // date
            central += u32(crc)
            central += u32(UInt32(body.count))
            central += u32(UInt32(body.count))
            central += u16(nameBuf.count)
            central += u16(0)  // extra len
            central += u16(0)  // comment len
            central += u16(0)  // disk number start
            central += u16(0)  // internal attrs
            central += u32(0)  // external attrs
            central += u32(UInt32(offset))
            centrals += central
            centrals += nameBuf

            offset += 30 + nameBuf.count + body.count
        }

        var eocd: [UInt8] = []
        eocd += u32(0x0605_4B50)
        eocd += u16(0) // disk number
        eocd += u16(0) // disk with central dir
        eocd += u16(entries.count)
        eocd += u16(entries.count)
        eocd += u32(UInt32(centrals.count))
        eocd += u32(UInt32(offset))
        eocd += u16(0) // comment len

        return locals + centrals + eocd
    }

    enum ZipError: Error, CustomStringConvertible {
        case notAZip
        case badCentralHeader
        case unsupportedMethod(Int, String)
        case crcMismatch(String)
        case inflateFailed(String)

        var description: String {
            switch self {
            case .notAZip: return "not a zip: EOCD missing"
            case .badCentralHeader: return "bad central header"
            case let .unsupportedMethod(m, name): return "unsupported zip method \(m) for \(name)"
            case let .crcMismatch(name): return "crc mismatch: \(name)"
            case let .inflateFailed(name): return "inflate failed: \(name)"
            }
        }
    }

    private static func readU16(_ b: [UInt8], _ p: Int) -> Int { Int(b[p]) | (Int(b[p + 1]) << 8) }
    private static func readU32(_ b: [UInt8], _ p: Int) -> Int {
        Int(b[p]) | (Int(b[p + 1]) << 8) | (Int(b[p + 2]) << 16) | (Int(b[p + 3]) << 24)
    }

    /// Reads a ZIP into name → bytes, verifying CRCs. Handles STORE and DEFLATE.
    static func read(_ buf: [UInt8]) throws -> [(name: String, data: [UInt8])] {
        var eocd = -1
        var i = buf.count - 22
        while i >= 0 {
            if readU32(buf, i) == 0x0605_4B50 { eocd = i; break }
            i -= 1
        }
        if eocd == -1 { throw ZipError.notAZip }
        let count = readU16(buf, eocd + 10)
        var p = readU32(buf, eocd + 16)
        var out: [(String, [UInt8])] = []
        for _ in 0..<count {
            if readU32(buf, p) != 0x0201_4B50 { throw ZipError.badCentralHeader }
            let method = readU16(buf, p + 10)
            let crc = UInt32(bitPattern: Int32(truncatingIfNeeded: readU32(buf, p + 16)))
            let csize = readU32(buf, p + 20)
            let usize = readU32(buf, p + 24)
            let nameLen = readU16(buf, p + 28)
            let extraLen = readU16(buf, p + 30)
            let commentLen = readU16(buf, p + 32)
            let localOff = readU32(buf, p + 42)
            let name = String(decoding: buf[(p + 46)..<(p + 46 + nameLen)], as: UTF8.self)
            let lNameLen = readU16(buf, localOff + 26)
            let lExtraLen = readU16(buf, localOff + 28)
            let dataStart = localOff + 30 + lNameLen + lExtraLen
            let raw = Array(buf[dataStart..<(dataStart + csize)])

            let data: [UInt8]
            if method == 0 {
                data = raw
            } else if method == 8 {
                data = try inflateRaw(raw, expected: usize, name: name)
            } else {
                throw ZipError.unsupportedMethod(method, name)
            }
            if crc32(data) != crc { throw ZipError.crcMismatch(name) }
            out.append((name, data))
            p += 46 + nameLen + extraLen + commentLen
        }
        return out
    }

    /// Inflates a raw DEFLATE stream via Compression (COMPRESSION_ZLIB == raw).
    static func inflateRaw(_ raw: [UInt8], expected: Int, name: String) throws -> [UInt8] {
        var capacity = max(expected, 64)
        for _ in 0..<8 {
            let dst = UnsafeMutablePointer<UInt8>.allocate(capacity: capacity)
            defer { dst.deallocate() }
            let written = raw.withUnsafeBufferPointer { src -> Int in
                compression_decode_buffer(dst, capacity, src.baseAddress!, src.count, nil, COMPRESSION_ZLIB)
            }
            if written > 0, written < capacity || written == expected {
                return Array(UnsafeBufferPointer(start: dst, count: written))
            }
            if written == capacity {
                capacity *= 2
                continue
            }
            if written > 0 {
                return Array(UnsafeBufferPointer(start: dst, count: written))
            }
            break
        }
        throw ZipError.inflateFailed(name)
    }
}
