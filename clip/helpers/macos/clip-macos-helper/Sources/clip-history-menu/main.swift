import AppKit
import Foundation
import SwiftUI

private let popoverSize = NSSize(width: 420, height: 520)

struct HistoryEntry: Identifiable {
    let id: String
    let mime: String
    let preview: String
    let imageURL: URL?
    let updatedAt: Date?
}

struct HistoryPage {
    let entries: [HistoryEntry]
    let totalCount: Int
}

struct HistoryIndexRow {
    let id: String
    let mime: String
    let preview: String
    let updatedAt: Date?
}

final class CommandRunner {
    private let clipHistoryCommand: [String]
    private let childEnvironment: [String: String]
    private let storeDirectory: URL
    init(environment: [String: String] = ProcessInfo.processInfo.environment) {
        var childEnvironment = environment
        let executable = URL(fileURLWithPath: CommandLine.arguments[0])
        let executableDirectory = executable.deletingLastPathComponent()
        let siblingHelper = executableDirectory.appendingPathComponent("clip-macos-helper")
        if childEnvironment["CLIP_MACOS_HELPER"] == nil,
            FileManager.default.isExecutableFile(atPath: siblingHelper.path)
        {
            childEnvironment["CLIP_MACOS_HELPER"] = siblingHelper.path
        }
        self.childEnvironment = childEnvironment
        self.storeDirectory = Self.resolveStoreDirectory(environment: environment)

        if let configured = environment["CLIP_HISTORY_BIN"], !configured.isEmpty {
            clipHistoryCommand = [configured]
            return
        }

        let sibling = executableDirectory.appendingPathComponent("clip-history")
        if FileManager.default.isExecutableFile(atPath: sibling.path) {
            clipHistoryCommand = [sibling.path]
            return
        }

        clipHistoryCommand = ["/usr/bin/env", "clip-history"]
    }

    func list(limit: Int, offset: Int, query: String) throws -> HistoryPage {
        let allEntries = try readIndexRows()
        let normalizedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let filteredEntries = normalizedQuery.isEmpty
            ? allEntries
            : allEntries.filter { entry in
                "\(entry.id) \(entry.mime) \(entry.preview)".lowercased().contains(normalizedQuery)
            }
        guard offset < filteredEntries.count else {
            return HistoryPage(entries: [], totalCount: filteredEntries.count)
        }
        let pageRows = filteredEntries.dropFirst(offset).prefix(limit)
        return HistoryPage(
            entries: pageRows.map(entry(from:)),
            totalCount: filteredEntries.count
        )
    }

    func select(entry: HistoryEntry) throws {
        _ = try run(arguments: ["select", entry.id])
    }

    private func readIndexRows() throws -> [HistoryIndexRow] {
        let indexURL = storeDirectory.appendingPathComponent("index.tsv")
        let content = try String(contentsOf: indexURL, encoding: .utf8)
        var entries: [HistoryIndexRow] = []
        for line in content.split(whereSeparator: \.isNewline) {
            if line.hasPrefix("#") {
                continue
            }
            let fields = line.split(separator: "\t", maxSplits: 5, omittingEmptySubsequences: false)
            guard fields.count == 6, let milliseconds = TimeInterval(fields[0]) else {
                continue
            }
            let id = String(fields[1])
            let mime = String(fields[3])
            entries.append(
                HistoryIndexRow(
                    id: id,
                    mime: mime,
                    preview: unescapeIndexField(String(fields[5])),
                    updatedAt: Date(timeIntervalSince1970: milliseconds / 1000)
                )
            )
        }
        return entries
    }

    private func entry(from row: HistoryIndexRow) -> HistoryEntry {
        HistoryEntry(
            id: row.id,
            mime: row.mime,
            preview: row.preview,
            imageURL: imageURL(id: row.id, mime: row.mime),
            updatedAt: row.updatedAt
        )
    }

    private func unescapeIndexField(_ value: String) -> String {
        var output = ""
        var iterator = value.makeIterator()
        while let character = iterator.next() {
            guard character == "\\" else {
                output.append(character)
                continue
            }
            switch iterator.next() {
            case "\\":
                output.append("\\")
            case "t":
                output.append("\t")
            case "n":
                output.append("\n")
            case "r":
                output.append("\r")
            case .some(let escaped):
                output.append("\\")
                output.append(escaped)
            case .none:
                output.append("\\")
            }
        }
        return output
    }

    private func imageURL(id: String, mime: String) -> URL? {
        guard mime.hasPrefix("image/") else {
            return nil
        }
        return variantURL(id: id, matching: { $0.hasPrefix("image/") })
    }

    private func variantURL(id: String, matching predicate: (String) -> Bool) -> URL? {
        let entryDirectory = resolveEntryDirectory(id: id)
        guard let meta = try? String(
            contentsOf: entryDirectory.appendingPathComponent("meta.txt"),
            encoding: .utf8
        ) else {
            return nil
        }
        for line in meta.split(whereSeparator: \.isNewline) {
            guard line.hasPrefix("variant=") else {
                continue
            }
            let value = line.dropFirst("variant=".count)
            let parts = value.split(separator: "|", maxSplits: 2, omittingEmptySubsequences: false)
            guard parts.count == 3, predicate(String(parts[0])) else {
                continue
            }
            let url = entryDirectory.appendingPathComponent(String(parts[1]))
            if FileManager.default.isReadableFile(atPath: url.path) {
                return url
            }
        }
        return nil
    }

    private func resolveEntryDirectory(id: String) -> URL {
        let entries = storeDirectory.appendingPathComponent("entries")
        let exact = entries.appendingPathComponent(id)
        if FileManager.default.fileExists(atPath: exact.path) {
            return exact
        }

        guard let names = try? FileManager.default.contentsOfDirectory(atPath: entries.path),
            let match = names.first(where: { $0.hasPrefix(id) })
        else {
            return exact
        }
        return entries.appendingPathComponent(match)
    }

    private static func resolveStoreDirectory(environment: [String: String]) -> URL {
        if let value = environment["CLIP_HISTORY_DIR"], !value.isEmpty {
            return URL(fileURLWithPath: value)
        }
        if let value = environment["XDG_STATE_HOME"], !value.isEmpty {
            return URL(fileURLWithPath: value).appendingPathComponent("clip/history")
        }
        if let value = environment["HOME"], !value.isEmpty {
            return URL(fileURLWithPath: value).appendingPathComponent(".local/state/clip/history")
        }
        return URL(fileURLWithPath: ".clip-history")
    }

    private func run(arguments: [String]) throws -> Data {
        let process = Process()
        let output = Pipe()
        let error = Pipe()

        process.executableURL = URL(fileURLWithPath: clipHistoryCommand[0])
        process.arguments = Array(clipHistoryCommand.dropFirst()) + arguments
        process.environment = childEnvironment
        process.standardOutput = output
        process.standardError = error

        try process.run()
        process.waitUntilExit()

        let outputData = output.fileHandleForReading.readDataToEndOfFile()
        if process.terminationStatus == 0 {
            return outputData
        }

        let errorData = error.fileHandleForReading.readDataToEndOfFile()
        let message = String(data: errorData, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let commandText = (clipHistoryCommand + arguments).joined(separator: " ")
        let logMessage = message?.isEmpty == false ? message! : "clip-history failed"
        fputs(
            "clip-history-menu: \(commandText) exited \(process.terminationStatus): \(logMessage)\n",
            stderr
        )
        throw NSError(domain: "clip-history-menu", code: Int(process.terminationStatus), userInfo: [
            NSLocalizedDescriptionKey: logMessage,
        ])
    }
}

protocol HistoryViewControllerDelegate: AnyObject {
    func historyViewControllerDidRequestRefresh(_ controller: HistoryViewController)
    func historyViewControllerDidRequestMore(_ controller: HistoryViewController)
    func historyViewController(_ controller: HistoryViewController, didSearch query: String)
    func historyViewController(_ controller: HistoryViewController, didSelect entry: HistoryEntry)
    func historyViewControllerDidRequestQuit(_ controller: HistoryViewController)
}

final class HistoryModel: ObservableObject {
    @Published var entries: [HistoryEntry] = []
    @Published var errorMessage: String?
    @Published var searchText = ""
    @Published var totalCount = 0
    @Published var hasMore = false
    @Published var isLoadingMore = false
    @Published var shouldFocusSearch = false

    var onRefresh: (() -> Void)?
    var onQuit: (() -> Void)?
    var onSelect: ((HistoryEntry) -> Void)?
    var onSearchChange: ((String) -> Void)?
    var onLoadMore: (() -> Void)?
}

final class HistoryViewController: NSHostingController<HistoryContentView> {
    let model: HistoryModel

    weak var delegate: HistoryViewControllerDelegate? {
        didSet {
            model.onRefresh = { [weak self] in
                guard let self else { return }
                self.delegate?.historyViewControllerDidRequestRefresh(self)
            }
            model.onQuit = { [weak self] in
                guard let self else { return }
                self.delegate?.historyViewControllerDidRequestQuit(self)
            }
            model.onSelect = { [weak self] entry in
                guard let self else { return }
                self.delegate?.historyViewController(self, didSelect: entry)
            }
            model.onSearchChange = { [weak self] query in
                guard let self else { return }
                self.delegate?.historyViewController(self, didSearch: query)
            }
            model.onLoadMore = { [weak self] in
                guard let self else { return }
                self.delegate?.historyViewControllerDidRequestMore(self)
            }
        }
    }

    init() {
        let model = HistoryModel()
        self.model = model
        super.init(rootView: HistoryContentView(model: model))
        preferredContentSize = popoverSize
    }

    @MainActor @preconcurrency required dynamic init?(coder: NSCoder) {
        nil
    }

    func update(
        entries: [HistoryEntry],
        totalCount: Int,
        errorMessage: String?,
        hasMore: Bool,
        isLoadingMore: Bool = false
    ) {
        model.entries = entries
        model.totalCount = totalCount
        model.errorMessage = errorMessage
        model.hasMore = hasMore
        model.isLoadingMore = isLoadingMore
    }

    func focusSearch() {
        model.shouldFocusSearch.toggle()
    }
}

struct HistoryContentView: View {
    @ObservedObject var model: HistoryModel

    var body: some View {
        VStack(spacing: 0) {
            header
            searchBar
            Divider()
            content
        }
        .frame(width: popoverSize.width, height: popoverSize.height)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("Clipboard History")
                .font(.system(size: 15, weight: .semibold))
            if let errorMessage = model.errorMessage, !errorMessage.isEmpty {
                Text("Error")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            } else {
                Text("\(model.totalCount) items")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button("Refresh") {
                model.onRefresh?()
            }
            Button("Quit") {
                model.onQuit?()
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 48)
    }

    private var searchBar: some View {
        SearchField(
            text: $model.searchText,
            focusSignal: model.shouldFocusSearch
        ) { value in
            model.onSearchChange?(value)
        }
            .frame(height: 24)
            .padding(.horizontal, 14)
            .padding(.bottom, 8)
    }

    @ViewBuilder private var content: some View {
        if let errorMessage = model.errorMessage {
            Text(errorMessage)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if model.entries.isEmpty {
            Text(model.searchText.isEmpty ? "No history yet" : "No matches")
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollViewReader { proxy in
                ZStack(alignment: .bottomTrailing) {
                    ScrollView {
                        LazyVStack(spacing: 0) {
                            ForEach(model.entries) { entry in
                                Button {
                                    model.onSelect?(entry)
                                } label: {
                                    HistoryRow(entry: entry)
                                }
                                .buttonStyle(.plain)
                                .id(entry.id)
                                Divider()
                                if entry.id == model.entries.last?.id, model.hasMore {
                                    loadMoreRow
                                }
                            }
                        }
                    }
                    if model.totalCount > 40 {
                        Button {
                            if let first = model.entries.first {
                                proxy.scrollTo(first.id, anchor: .top)
                            }
                        } label: {
                            Image(systemName: "arrow.up")
                                .font(.system(size: 13, weight: .semibold))
                                .frame(width: 28, height: 28)
                        }
                        .buttonStyle(.bordered)
                        .clipShape(Circle())
                        .padding(12)
                    }
                }
            }
        }
    }

    private var loadMoreRow: some View {
        HStack {
            Spacer()
            if model.isLoadingMore {
                ProgressView()
                    .controlSize(.small)
            } else {
                Text("Loading more...")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .frame(height: 34)
        .onAppear {
            model.onLoadMore?()
        }
    }
}

struct SearchField: NSViewRepresentable {
    @Binding var text: String
    let focusSignal: Bool
    let onChange: (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text, onChange: onChange)
    }

    func makeNSView(context: Context) -> SelectAllSearchField {
        let field = SelectAllSearchField()
        field.placeholderString = "Search"
        field.delegate = context.coordinator
        field.sendsSearchStringImmediately = true
        field.focusRingType = .default
        return field
    }

    func updateNSView(_ nsView: SelectAllSearchField, context: Context) {
        if nsView.stringValue != text {
            nsView.stringValue = text
        }
        if context.coordinator.focusSignal != focusSignal {
            context.coordinator.focusSignal = focusSignal
            DispatchQueue.main.async {
                nsView.window?.makeFirstResponder(nsView)
            }
        }
    }

    final class Coordinator: NSObject, NSSearchFieldDelegate {
        @Binding var text: String
        let onChange: (String) -> Void
        var focusSignal = false

        init(text: Binding<String>, onChange: @escaping (String) -> Void) {
            _text = text
            self.onChange = onChange
        }

        func controlTextDidChange(_ obj: Notification) {
            guard let field = obj.object as? NSSearchField else {
                return
            }
            text = field.stringValue
            onChange(field.stringValue)
        }
    }
}

final class SelectAllSearchField: NSSearchField {
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(.command),
            event.charactersIgnoringModifiers?.lowercased() == "a"
        {
            if let editor = currentEditor() {
                editor.selectAll(nil)
            } else {
                selectText(nil)
            }
            return true
        }
        return super.performKeyEquivalent(with: event)
    }
}

struct HistoryRow: View {
    let entry: HistoryEntry
    private static let dateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        return formatter
    }()

    var body: some View {
        Group {
            if let imageURL = entry.imageURL, let image = NSImage(contentsOf: imageURL) {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(maxWidth: .infinity)
                    .frame(height: 108)
                    .padding(.vertical, 8)
                    .padding(.horizontal, 14)
            } else {
                VStack(alignment: .leading, spacing: 4) {
                    Text(entry.preview.isEmpty ? entry.mime : entry.preview)
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(.primary)
                        .lineLimit(2)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text(updatedAtText)
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .padding(.horizontal, 18)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: 58)
            }
        }
        .contentShape(Rectangle())
    }

    private var updatedAtText: String {
        guard let updatedAt = entry.updatedAt else {
            return "unknown"
        }
        return Self.dateFormatter.string(from: updatedAt)
    }
}

final class MenuController: NSObject, NSApplicationDelegate, HistoryViewControllerDelegate {
    private let runner = CommandRunner()
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    private let popover = NSPopover()
    private let historyViewController = HistoryViewController()
    private let pageSize = 40
    private var lastError: String?
    private var currentQuery = ""
    private var isLoadingMore = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        statusItem.button?.title = "Clip"
        statusItem.button?.toolTip = "Clipboard History"
        statusItem.button?.target = self
        statusItem.button?.action = #selector(togglePopover)

        historyViewController.delegate = self
        popover.contentViewController = historyViewController
        popover.contentSize = popoverSize
        popover.behavior = .transient
        popover.animates = true
    }

    private func reloadHistory(reset: Bool) {
        if reset {
            historyViewController.update(entries: [], totalCount: 0, errorMessage: nil, hasMore: false)
        }
        do {
            let page = try runner.list(limit: pageSize, offset: 0, query: currentQuery)
            lastError = nil
            historyViewController.update(
                entries: page.entries,
                totalCount: page.totalCount,
                errorMessage: nil,
                hasMore: page.entries.count < page.totalCount
            )
        } catch {
            lastError = error.localizedDescription
            historyViewController.update(
                entries: [],
                totalCount: 0,
                errorMessage: error.localizedDescription,
                hasMore: false
            )
        }
    }

    private func loadMoreHistory() {
        guard !isLoadingMore, historyViewController.model.hasMore else {
            return
        }
        isLoadingMore = true
        historyViewController.model.isLoadingMore = true
        do {
            let existingEntries = historyViewController.model.entries
            let page = try runner.list(
                limit: pageSize,
                offset: existingEntries.count,
                query: currentQuery
            )
            historyViewController.update(
                entries: existingEntries + page.entries,
                totalCount: page.totalCount,
                errorMessage: nil,
                hasMore: existingEntries.count + page.entries.count < page.totalCount
            )
            lastError = nil
        } catch {
            lastError = error.localizedDescription
            historyViewController.update(
                entries: historyViewController.model.entries,
                totalCount: historyViewController.model.totalCount,
                errorMessage: error.localizedDescription,
                hasMore: false
            )
        }
        isLoadingMore = false
    }

    @objc private func togglePopover() {
        guard let button = statusItem.button else {
            return
        }
        if popover.isShown {
            popover.performClose(nil)
            return
        }
        reloadHistory(reset: true)
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        NSApp.activate(ignoringOtherApps: true)
        popover.contentViewController?.view.window?.makeKey()
        historyViewController.focusSearch()
    }

    func historyViewControllerDidRequestRefresh(_ controller: HistoryViewController) {
        reloadHistory(reset: true)
    }

    func historyViewControllerDidRequestMore(_ controller: HistoryViewController) {
        loadMoreHistory()
    }

    func historyViewController(_ controller: HistoryViewController, didSearch query: String) {
        currentQuery = query
        reloadHistory(reset: true)
    }

    func historyViewController(_ controller: HistoryViewController, didSelect entry: HistoryEntry) {
        let startedAt = Date()
        do {
            try runner.select(entry: entry)
            let elapsed = Date().timeIntervalSince(startedAt)
            if elapsed > 0.5 {
                fputs(
                    "clip-history-menu: select \(entry.id) took \(String(format: "%.2f", elapsed))s\n",
                    stderr
                )
            }
            lastError = nil
            reloadHistory(reset: false)
            popover.performClose(nil)
            flashStatusTitle("Copied")
        } catch {
            fputs("clip-history-menu: select \(entry.id) failed: \(error.localizedDescription)\n", stderr)
            lastError = error.localizedDescription
            historyViewController.update(
                entries: [],
                totalCount: 0,
                errorMessage: error.localizedDescription,
                hasMore: false
            )
            flashStatusTitle("Clip!")
        }
    }

    func historyViewControllerDidRequestQuit(_ controller: HistoryViewController) {
        NSApp.terminate(nil)
    }

    private func flashStatusTitle(_ title: String) {
        let original = statusItem.button?.title ?? "Clip"
        statusItem.button?.title = title
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
            self?.statusItem.button?.title = original
        }
    }
}

let app = NSApplication.shared
let delegate = MenuController()
app.delegate = delegate
app.run()
