import XCTest
@testable import BooksExporterCore

/// Covers the rule ADR 0007 is strictest about: the picker resolves a
/// constrained choice into one already-resolved `voice_id`, and anything
/// unusable is shown as unusable rather than replaced.
final class SpeechVoiceCatalogTests: XCTestCase {

    private func voice(
        _ id: String,
        name: String?,
        emotion: String? = nil,
        style: String? = nil,
        description: [String] = [],
        provider: String = "senseaudio",
        sourceType: String = "system"
    ) -> VoiceCatalogVoice {
        VoiceCatalogVoice(
            provider: provider,
            sourceType: sourceType,
            voiceID: id,
            voiceName: name,
            emotionLabel: emotion,
            styleLabel: style,
            description: description,
            createdTime: nil
        )
    }

    private func receipt(_ voices: [VoiceCatalogVoice], stale: Bool = false) -> VoiceCatalogReceipt {
        // Decoding through the real DTO keeps this from drifting away from the
        // contract: a fixture built by hand could assert behaviour the actual
        // wire format does not support.
        let json = """
        {"schema_version":1,"receipt":{"operation":"voices","provider":"senseaudio",
         "fetched_at":"2026-09-30T07:38:38+00:00","stale":\(stale),
         "voices":[\(voices.map(Self.encode).joined(separator: ","))],"warnings":[]}}
        """
        return try! JSONDecoder()
            .decode(VoiceCatalogResponse.self, from: Data(json.utf8))
            .receipt
    }

    private static func encode(_ v: VoiceCatalogVoice) -> String {
        func field(_ value: String?) -> String {
            guard let value else { return "null" }
            return "\"\(value)\""
        }
        // An empty array must emit `[]`, not an empty slot: the contract sends
        // `[]` for a voice with no variants, and joining an empty list to ""
        // produces `[]` -> `""` -> invalid JSON.
        let description = v.description.isEmpty
            ? "[]"
            : "[\(v.description.map { "\"\($0)\"" }.joined(separator: ","))]"
        return """
        {"provider":"\(v.provider)","source_type":"\(v.sourceType)","voice_id":"\(v.voiceID)",
         "voice_name":\(field(v.voiceName)),"emotion_label":\(field(v.emotionLabel)),
         "style_label":\(field(v.styleLabel)),
         "description":\(description),"created_time":null}
        """
    }

    // MARK: - Grouping

    func testVariantsGroupUnderTheVoiceTheyBelongTo() {
        let catalog = SpeechVoiceCatalogBuilder.build(from: receipt([
            voice("female_0033_a", name: "女声 0033", emotion: "平静", description: ["平静"]),
            voice("female_0033_b", name: "女声 0033", emotion: "开心", description: ["开心"]),
            voice("male_0004_a", name: "男声 0004", emotion: nil, description: []),
        ]))

        XCTAssertEqual(catalog.groups.count, 2)
        // Order follows the provider's own listing rather than being re-sorted.
        XCTAssertEqual(catalog.groups.map(\.name), ["女声 0033", "男声 0004"])
        XCTAssertEqual(catalog.groups[0].variants.map(\.voiceID),
                       ["female_0033_a", "female_0033_b"])
    }

    func testSubtitleCarriesBothLabelsWhenTheCatalogProvidesBoth() {
        let catalog = SpeechVoiceCatalogBuilder.build(from: receipt([
            voice("v1", name: "女声", emotion: "开心", style: "轻快", description: ["开心"]),
        ]))

        XCTAssertEqual(catalog.groups[0].variants[0].option.subtitle, "开心 · 轻快")
    }

    func testVoiceWithoutANameFallsBackToItsIDRatherThanShowingABlankRow() {
        let catalog = SpeechVoiceCatalogBuilder.build(from: receipt([
            voice("female_0033_b", name: nil, emotion: "开心", description: ["开心"]),
        ]))

        XCTAssertEqual(catalog.groups.map(\.name), ["female_0033_b"])
    }

    func testADescriptionOfEmptyIsNotReadAsMissing() {
        // The contract sends `[]` rather than null, and the two mean different
        // things: no variants, versus a field the app failed to decode.
        let catalog = SpeechVoiceCatalogBuilder.build(from: receipt([
            voice("male_0004_a", name: "男声 0004", emotion: nil, description: []),
        ]))

        XCTAssertEqual(catalog.groups[0].variants[0].option.description, [])
        XCTAssertEqual(catalog.groups[0].variants[0].option.subtitle, "")
    }

    // MARK: - Only the exercised provider is presented

    func testVoicesFromAnotherProviderAreNotOffered() {
        // ADR 0007 makes SenseAudio the first provider, not a choice among
        // verified ones. A row for an unexercised provider would claim a path
        // the app has never taken.
        let catalog = SpeechVoiceCatalogBuilder.build(from: receipt([
            voice("a1", name: "SenseAudio 音色", provider: "senseaudio"),
            voice("b1", name: "别家音色", provider: "someone-else"),
        ]))

        XCTAssertEqual(catalog.groups.map(\.name), ["SenseAudio 音色"])
    }

    // MARK: - Unavailable stays unavailable

    func testUnavailableVariantsAreKeptAndLabelledRatherThanDropped() {
        let catalog = SpeechVoiceCatalogBuilder.build(
            from: receipt([
                voice("female_0033_a", name: "女声 0033", emotion: "平静", description: ["平静"]),
                voice("female_0033_b", name: "女声 0033", emotion: "开心", description: ["开心"]),
            ]),
            unavailableReason: "该音色当前账号不可用"
        )

        let variants = catalog.groups[0].variants
        XCTAssertEqual(variants.count, 2, "不可用的条目必须留在列表里并说明原因，而不是消失")
        XCTAssertTrue(variants.allSatisfy { !$0.isAvailable })
        guard case .unavailable(let reason) = variants[0].availability else {
            return XCTFail("expected an unavailable availability")
        }
        XCTAssertEqual(reason, "该音色当前账号不可用")
        XCTAssertFalse(catalog.groups[0].isUsable)
    }

    // MARK: - The default voice is never substituted

    func testStoredVoiceIsLocatedWhenTheCatalogStillHasIt() {
        let catalog = SpeechVoiceCatalogBuilder.build(from: receipt([
            voice("female_0033_b", name: "女声 0033", emotion: "开心", description: ["开心"]),
            voice("male_0004_a", name: "男声 0004", emotion: nil, description: []),
        ]))

        let group = catalog.group(containing: "male_0004_a")
        XCTAssertEqual(group?.name, "男声 0004")
        XCTAssertEqual(catalog.groups.firstIndex(of: group!), 1)
    }

    func testAStoredVoiceMissingFromTheCatalogResolvesToNothing() {
        // The important half: nil means "ask the user", whereas quietly
        // returning the first group would be the silent substitution ADR 0007
        // forbids at generation time.
        let catalog = SpeechVoiceCatalogBuilder.build(from: receipt([
            voice("female_0033_b", name: "女声 0033", emotion: "开心", description: ["开心"]),
        ]))

        XCTAssertNil(catalog.group(containing: "male_0004_a"))
        XCTAssertNil(catalog.group(containing: nil))
    }

    func testAnEmptyCatalogIsReportedAsEmptyRatherThanFailing() {
        let catalog = SpeechVoiceCatalogBuilder.build(from: receipt([]))

        XCTAssertTrue(catalog.isEmpty)
        XCTAssertTrue(catalog.groups.isEmpty)
    }
}
