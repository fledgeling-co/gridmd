import XCTest
import Compression
@testable import GridMD

final class ZipTests: XCTestCase {
    func testWriteReadRoundTrip() throws {
        let entries = [
            ZipEntry(name: "a.txt", data: Array("hello".utf8)),
            ZipEntry(name: "dir/b.bin", data: [0, 1, 2, 3, 255]),
        ]
        let bytes = Zip.write(entries)
        let read = try Zip.read(bytes)
        XCTAssertEqual(read.count, 2)
        XCTAssertEqual(read[0].name, "a.txt")
        XCTAssertEqual(String(decoding: read[0].data, as: UTF8.self), "hello")
        XCTAssertEqual(read[1].data, [0, 1, 2, 3, 255])
    }

    func testCrc32() {
        XCTAssertEqual(crc32(Array("The quick brown fox jumps over the lazy dog".utf8)), 0x414F_A339)
        XCTAssertEqual(crc32([]), 0)
    }

    func testDeflateEntry() throws {
        let payload = Array("deflate me ".utf8) + Array(repeating: UInt8(65), count: 500)
        let zip = makeDeflateZip(name: "d.txt", payload: payload)
        let read = try Zip.read(zip)
        XCTAssertEqual(read.first?.data, payload)
    }

    func testInflateRawDirect() throws {
        let payload = Array(repeating: UInt8(66), count: 200)
        let comp = rawDeflate(payload)
        let back = try Zip.inflateRaw(comp, expected: payload.count, name: "x")
        XCTAssertEqual(back, payload)
        // grows the buffer when `expected` under-estimates
        let back2 = try Zip.inflateRaw(comp, expected: 1, name: "x")
        XCTAssertEqual(back2, payload)
    }

    func testReadErrors() {
        XCTAssertThrowsError(try Zip.read([1, 2, 3])) // no EOCD
        XCTAssertThrowsError(try Zip.read(makeMethodZip(method: 99))) // unsupported method
        var store = Zip.write([ZipEntry(name: "a", data: Array("hello".utf8))])
        // corrupt the stored data byte (local data starts at offset 30 + nameLen)
        store[30 + 1] ^= 0xFF
        XCTAssertThrowsError(try Zip.read(store)) // crc mismatch
    }

    // MARK: helpers

    func rawDeflate(_ src: [UInt8]) -> [UInt8] {
        let cap = src.count + 128
        let dst = UnsafeMutablePointer<UInt8>.allocate(capacity: cap)
        defer { dst.deallocate() }
        let n = src.withUnsafeBufferPointer { s in
            compression_encode_buffer(dst, cap, s.baseAddress!, s.count, nil, COMPRESSION_ZLIB)
        }
        return Array(UnsafeBufferPointer(start: dst, count: n))
    }

    func le16(_ v: Int) -> [UInt8] { [UInt8(v & 0xFF), UInt8((v >> 8) & 0xFF)] }
    func le32(_ v: UInt32) -> [UInt8] { [UInt8(v & 0xFF), UInt8((v >> 8) & 0xFF), UInt8((v >> 16) & 0xFF), UInt8((v >> 24) & 0xFF)] }

    func makeDeflateZip(name: String, payload: [UInt8]) -> [UInt8] {
        let comp = rawDeflate(payload)
        return assembleZip(name: name, method: 8, body: comp, crc: crc32(payload), usize: payload.count)
    }

    func makeMethodZip(method: Int) -> [UInt8] {
        let body = Array("x".utf8)
        return assembleZip(name: "a", method: method, body: body, crc: crc32(body), usize: body.count)
    }

    func assembleZip(name: String, method: Int, body: [UInt8], crc: UInt32, usize: Int) -> [UInt8] {
        let nameBuf = Array(name.utf8)
        var local: [UInt8] = []
        local += le32(0x0403_4B50); local += le16(20); local += le16(0); local += le16(method)
        local += le16(0); local += le16(0x21); local += le32(crc); local += le32(UInt32(body.count))
        local += le32(UInt32(usize)); local += le16(nameBuf.count); local += le16(0)
        let localTotal = local + nameBuf + body

        var central: [UInt8] = []
        central += le32(0x0201_4B50); central += le16(20); central += le16(20); central += le16(0)
        central += le16(method); central += le16(0); central += le16(0x21); central += le32(crc)
        central += le32(UInt32(body.count)); central += le32(UInt32(usize)); central += le16(nameBuf.count)
        central += le16(0); central += le16(0); central += le16(0); central += le16(0)
        central += le32(0); central += le32(0)
        let centralTotal = central + nameBuf

        var eocd: [UInt8] = []
        eocd += le32(0x0605_4B50); eocd += le16(0); eocd += le16(0); eocd += le16(1); eocd += le16(1)
        eocd += le32(UInt32(centralTotal.count)); eocd += le32(UInt32(localTotal.count)); eocd += le16(0)
        return localTotal + centralTotal + eocd
    }
}

final class XlsxTests: XCTestCase {
    let doc = """
    ---
    gridmd: "1.0"
    ---
    # S
    @ A1 "text & <stuff>"
    @ A2 42
    @ A3 TRUE
    @ A4 #DIV/0!
    @ A5 2026-07-04
    @ A6 =SUM(B:B) :: 5
    @ A7 =CONCAT(B1) :: "hi"
    @ A8 =SUBTOTAL(9,B:B)
    @ A10 =ISBLANK(B1) :: TRUE
    @ A11 =TODAY() :: 2026-07-04
    @ A9:C9 { merge: true }
    @ A9 "merged"
    @ B1
      rich:
        - { text: "R" }
    """

    func testExportImportRoundTrip() throws {
        let exported = try GridMD.exportXLSX(doc)
        let imported = try GridMD.importXLSX(exported.data)
        XCTAssertEqual(imported.gmd, doc)
        XCTAssertTrue(exported.report.contains { $0.action == "carried" })
    }

    func testWorksheetXmlBranches() throws {
        let exported = try GridMD.exportXLSX(doc)
        let parts = try Zip.read(Array(exported.data))
        let sheet = parts.first { $0.name == "xl/worksheets/sheet1.xml" }!
        let xml = String(decoding: sheet.data, as: UTF8.self)
        XCTAssertTrue(xml.contains("text &amp; &lt;stuff&gt;")) // escaped inlineStr
        XCTAssertTrue(xml.contains("<v>42</v>"))
        XCTAssertTrue(xml.contains("t=\"b\"><v>1</v>"))
        XCTAssertTrue(xml.contains("t=\"e\"><v>#DIV/0!</v>"))
        XCTAssertTrue(xml.contains("<f>SUM(B:B)</f><v>5</v>"))
        XCTAssertTrue(xml.contains("t=\"str\"><f>CONCAT(B1)</f><v>hi</v>"))
        XCTAssertTrue(xml.contains("<f>SUBTOTAL(9,B:B)</f></c>")) // no cached
        XCTAssertTrue(xml.contains("t=\"b\"><f>ISBLANK(B1)</f><v>1</v>")) // boolean cached
        XCTAssertTrue(xml.contains("t=\"str\"><f>TODAY()</f><v>2026-07-04</v>")) // date cached
        XCTAssertTrue(xml.contains("<mergeCells count=\"1\"><mergeCell ref=\"A9:C9\"/>"))
        XCTAssertTrue(parts.contains { $0.name == "[Content_Types].xml" })
    }

    func testImportErrors() {
        // a zip without the carry part
        let noCarry = Zip.write([ZipEntry(name: "x.xml", data: Array("<x/>".utf8))])
        XCTAssertThrowsError(try GridMD.importXLSX(Data(noCarry))) { error in
            guard case GridMD.Failure.badXLSX = error else { return XCTFail() }
        }
        // a carry part with invalid base64
        let badB64 = Zip.write([ZipEntry(name: Xlsx.carryPart, data: Array("<gridmdCarry>!!!not base64!!!</gridmdCarry>".utf8))])
        XCTAssertThrowsError(try GridMD.importXLSX(Data(badB64)))
        // a carry part with no closing tag
        let noClose = Zip.write([ZipEntry(name: Xlsx.carryPart, data: Array("<gridmdCarry>QQ==".utf8))])
        XCTAssertThrowsError(try GridMD.importXLSX(Data(noClose)))
        // not a zip at all
        XCTAssertThrowsError(try GridMD.importXLSX(Data([1, 2, 3])))
    }
}

final class FacadeTests: XCTestCase {
    func testDumpThrowsInvalid() {
        XCTAssertThrowsError(try GridMD.dump("not gridmd")) { error in
            guard case let GridMD.Failure.invalid(diags) = error else { return XCTFail() }
            XCTAssertFalse(diags.isEmpty)
        }
    }

    func testExportThrowsInvalid() {
        XCTAssertThrowsError(try GridMD.exportXLSX("not gridmd"))
    }

    func testLintLenient() {
        let result = GridMD.lint("---\ngridmd: \"1.0\"\n---\n# S\nbogus", strict: false)
        XCTAssertTrue(result.isValid)
        XCTAssertFalse(result.warnings.isEmpty)
        XCTAssertEqual(result.sheets, 1)
    }

    func testFailureDescriptions() {
        XCTAssertTrue(GridMD.Failure.invalid([]).description.contains("invalid"))
        XCTAssertTrue(GridMD.Failure.badXLSX("x").description.contains("bad .xlsx"))
    }
}
