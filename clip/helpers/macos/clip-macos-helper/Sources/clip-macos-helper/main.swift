import AppKit
import Foundation

enum HelperError: Error {
    case usage(String)
    case clipboard(String)
}

func mimeToPasteboardType(_ mime: String) -> NSPasteboard.PasteboardType {
    switch mime {
    case "text/plain":
        return .string
    case "text/html":
        return NSPasteboard.PasteboardType("public.html")
    case "image/png":
        return .png
    case "text/uri-list":
        return NSPasteboard.PasteboardType("public.file-url")
    default:
        return NSPasteboard.PasteboardType(mime)
    }
}

func readStdin() -> Data {
    FileHandle.standardInput.readDataToEndOfFile()
}

func writeStdout(_ data: Data) {
    FileHandle.standardOutput.write(data)
}

func readUriList() throws -> Data {
    guard let values = pasteboard.readObjects(forClasses: [NSURL.self], options: nil) as? [NSURL],
        !values.isEmpty
    else {
        throw HelperError.clipboard("text/uri-list is not available in the clipboard")
    }

    let text = values
        .map { ($0 as URL).absoluteString }
        .joined(separator: "\n")
    return Data(text.utf8)
}

func parseUriList(_ payload: Data) throws -> [NSURL] {
    guard let text = String(data: payload, encoding: .utf8) else {
        throw HelperError.clipboard("text/uri-list payload must be valid UTF-8")
    }

    let urls = try text
        .split(whereSeparator: { $0.isNewline })
        .map(String.init)
        .filter { !$0.isEmpty && !$0.hasPrefix("#") }
        .map { value throws -> NSURL in
            guard let url = URL(string: value) else {
                throw HelperError.clipboard("text/uri-list contained an invalid URL")
            }
            return url as NSURL
        }

    guard !urls.isEmpty else {
        throw HelperError.clipboard("text/uri-list payload did not contain any URLs")
    }

    return urls
}

func parseUtf8Text(_ payload: Data, mime: String) throws -> String {
    guard let text = String(data: payload, encoding: .utf8) else {
        throw HelperError.clipboard("\(mime) payload must be valid UTF-8")
    }
    return text
}

func htmlPlainTextFallback(_ payload: Data) -> String {
    if let attributed = try? NSAttributedString(
        data: payload,
        options: [
            .documentType: NSAttributedString.DocumentType.html,
            .characterEncoding: String.Encoding.utf8.rawValue,
        ],
        documentAttributes: nil
    ) {
        let text = attributed.string.trimmingCharacters(in: .whitespacesAndNewlines)
        if !text.isEmpty {
            return text
        }
    }

    guard let html = String(data: payload, encoding: .utf8) else {
        return ""
    }
    let withoutTags = html.replacingOccurrences(
        of: "<[^>]+>",
        with: " ",
        options: .regularExpression
    )
    return withoutTags
        .components(separatedBy: .whitespacesAndNewlines)
        .filter { !$0.isEmpty }
        .joined(separator: " ")
}

func setPasteboardValue(_ item: NSPasteboardItem, mime: String, payload: Data) throws {
    let type = mimeToPasteboardType(mime)

    if mime == "text/plain" {
        let text = try parseUtf8Text(payload, mime: mime)
        guard item.setString(text, forType: .string) else {
            throw HelperError.clipboard("failed to prepare text/plain")
        }
        return
    }

    if mime == "text/html" {
        _ = try parseUtf8Text(payload, mime: mime)
        guard item.setData(payload, forType: type) else {
            throw HelperError.clipboard("failed to prepare \(mime)")
        }
        if item.availableType(from: [.string]) == nil {
            guard item.setString(htmlPlainTextFallback(payload), forType: .string) else {
                throw HelperError.clipboard("failed to prepare text/plain fallback for \(mime)")
            }
        }
        return
    }

    guard item.setData(payload, forType: type) else {
        throw HelperError.clipboard("failed to prepare \(mime)")
    }
}

func makePasteboardItem(mime: String, payload: Data) throws -> NSPasteboardItem {
    let item = NSPasteboardItem()
    try setPasteboardValue(item, mime: mime, payload: payload)
    return item
}

func setPasteboardValue(_ pasteboard: NSPasteboard, mime: String, payload: Data, hasPlainText: Bool) throws {
    let type = mimeToPasteboardType(mime)

    if mime == "text/plain" {
        let text = try parseUtf8Text(payload, mime: mime)
        guard pasteboard.setString(text, forType: .string) else {
            throw HelperError.clipboard("failed to write text/plain")
        }
        return
    }

    if mime == "text/html" {
        _ = try parseUtf8Text(payload, mime: mime)
        guard pasteboard.setData(payload, forType: type) else {
            throw HelperError.clipboard("failed to write \(mime)")
        }
        if !hasPlainText {
            guard pasteboard.setString(htmlPlainTextFallback(payload), forType: .string) else {
                throw HelperError.clipboard("failed to write text/plain fallback for \(mime)")
            }
        }
        return
    }

    guard pasteboard.setData(payload, forType: type) else {
        throw HelperError.clipboard("failed to write \(mime)")
    }
}

struct ClipboardVariant {
    let mime: String
    let payload: Data
}

func parseBundle(_ payload: Data) throws -> [ClipboardVariant] {
    var offset = payload.startIndex

    func readLine() -> String? {
        guard offset < payload.endIndex else {
            return nil
        }
        guard let newline = payload[offset...].firstIndex(of: 0x0a) else {
            return nil
        }
        let lineData = payload[offset..<newline]
        offset = payload.index(after: newline)
        return String(data: lineData, encoding: .utf8)
    }

    guard readLine() == "clip-bundle-v1" else {
        throw HelperError.clipboard("clipboard bundle has invalid header")
    }

    var variants: [ClipboardVariant] = []
    while offset < payload.endIndex {
        guard let mime = readLine(), !mime.isEmpty else {
            throw HelperError.clipboard("clipboard bundle has invalid MIME type")
        }
        guard let lengthLine = readLine(), let length = Int(lengthLine), length >= 0 else {
            throw HelperError.clipboard("clipboard bundle has invalid payload length")
        }
        guard payload.distance(from: offset, to: payload.endIndex) >= length else {
            throw HelperError.clipboard("clipboard bundle ended before payload")
        }
        let end = payload.index(offset, offsetBy: length)
        variants.append(ClipboardVariant(mime: mime, payload: payload[offset..<end]))
        offset = end
        if offset < payload.endIndex {
            guard payload[offset] == 0x0a else {
                throw HelperError.clipboard("clipboard bundle payload is missing separator")
            }
            offset = payload.index(after: offset)
        }
    }

    guard !variants.isEmpty else {
        throw HelperError.clipboard("clipboard bundle has no variants")
    }
    return variants
}

func requireType(_ args: [String]) throws -> String {
    guard let index = args.firstIndex(of: "--type"), args.indices.contains(index + 1) else {
        throw HelperError.usage("missing --type")
    }
    return args[index + 1]
}

let args = Array(CommandLine.arguments.dropFirst())
let pasteboard = NSPasteboard.general

do {
    guard let command = args.first else {
        throw HelperError.usage("expected one of: types, read, write, write-bundle")
    }

    switch command {
    case "types":
        let values = (pasteboard.types ?? []).map(\.rawValue).joined(separator: "\n")
        writeStdout(Data(values.utf8))
    case "read":
        let mime = try requireType(args)
        let type = mimeToPasteboardType(mime)
        if mime == "text/plain" {
            guard let text = pasteboard.string(forType: .string) else {
                throw HelperError.clipboard("text/plain is not available in the clipboard")
            }
            writeStdout(Data(text.utf8))
        } else if mime == "text/uri-list" {
            writeStdout(try readUriList())
        } else {
            guard let data = pasteboard.data(forType: type) else {
                throw HelperError.clipboard("\(mime) is not available in the clipboard")
            }
            writeStdout(data)
        }
    case "write":
        let mime = try requireType(args)
        let payload = readStdin()
        if mime == "text/uri-list" {
            let text = try parseUtf8Text(payload, mime: mime)
            let urls = try parseUriList(payload)
            pasteboard.clearContents()
            guard pasteboard.writeObjects(urls) else {
                throw HelperError.clipboard("failed to write \(mime)")
            }
            guard pasteboard.setString(text, forType: .string) else {
                throw HelperError.clipboard("failed to write text/plain fallback for \(mime)")
            }
        } else {
            let item = try makePasteboardItem(mime: mime, payload: payload)
            pasteboard.clearContents()
            guard pasteboard.writeObjects([item]) else {
                throw HelperError.clipboard("failed to write \(mime)")
            }
        }
    case "write-bundle":
        let variants = try parseBundle(readStdin())
        let orderedVariants = variants.sorted { left, right in
            if left.mime == "text/plain" {
                return right.mime != "text/plain"
            }
            if right.mime == "text/plain" {
                return false
            }
            return false
        }
        let declaredTypes = orderedVariants
            .map { mimeToPasteboardType($0.mime) }
            .reduce(into: [NSPasteboard.PasteboardType]()) { values, type in
                if !values.contains(type) {
                    values.append(type)
                }
            }
        let hasPlainText = orderedVariants.contains { $0.mime == "text/plain" }
        var pasteboardTypes = declaredTypes
        if !hasPlainText, orderedVariants.contains(where: { $0.mime == "text/html" }),
            !pasteboardTypes.contains(.string)
        {
            pasteboardTypes.append(.string)
        }
        pasteboard.clearContents()
        pasteboard.declareTypes(pasteboardTypes, owner: nil)
        for variant in orderedVariants {
            try setPasteboardValue(
                pasteboard,
                mime: variant.mime,
                payload: variant.payload,
                hasPlainText: hasPlainText
            )
        }
    default:
        throw HelperError.usage("expected one of: types, read, write, write-bundle")
    }
} catch let error as HelperError {
    switch error {
    case .usage(let message), .clipboard(let message):
        fputs("\(message)\n", stderr)
        exit(1)
    }
} catch {
    fputs("\(error)\n", stderr)
    exit(1)
}
