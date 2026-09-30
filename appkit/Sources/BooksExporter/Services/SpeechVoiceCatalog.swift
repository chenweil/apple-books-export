import Foundation

/// A voice as the user picks it: a name, and the variants that name actually
/// supports.
///
/// The two levels exist because the contract resolves emotion and style into
/// the `voice_id` itself. SenseAudio encodes them in the variant
/// (`female_0033_b`), and ADR 0007 requires the GUI to turn a constrained
/// selection into an already-resolved ID and the machine call to submit only
/// that ID -- no fuzzy matching on names or labels. So each catalog entry is
/// one fully resolved combination, and the picker's job is to present the
/// combinations a chosen voice supports without implying the provider has
/// separate emotion or style parameters.
///
/// Grouping is by `voice_name` because the catalog has no base-voice field.
/// That is a display grouping over a provider-supplied string, not a provider
/// identity: it is the only grouping key the contract offers, and if a
/// provider ever returns two unrelated voices under one name they will appear
/// as variants of each other. The alternative -- flattening to one list --
/// would drop the "this voice supports *these*" relationship that ADR 0007
/// asks the picker to show.
struct SpeechVoiceOption: Equatable, Identifiable {
    let voiceID: String
    let displayName: String
    let emotionLabel: String?
    let styleLabel: String?
    let sourceType: String
    /// Empty rather than nil: the contract sends `description: []` for a
    /// voice with no variants, and a missing array must not read as "unknown".
    let description: [String]

    var id: String { voiceID }

    /// What the row says. Both labels are shown when present because a catalog
    /// entry can carry one, the other, or both.
    var subtitle: String {
        var parts: [String] = []
        if let emotionLabel, !emotionLabel.isEmpty { parts.append(emotionLabel) }
        if let styleLabel, !styleLabel.isEmpty { parts.append(styleLabel) }
        return parts.joined(separator: " · ")
    }
}

/// A resolved (voice, emotion, style) combination together with whether it can
/// be used.
enum SpeechVariantAvailability: Equatable {
    case available
    /// Explicitly unusable, with the reason shown. ADR 0007 forbids silently
    /// substituting a nearby voice, so an unavailable entry stays in the list
    /// and says why rather than disappearing or being swapped.
    case unavailable(reason: String)
}

/// One selectable row: the resolved `voice_id` plus how to label it.
struct SpeechVariantOption: Equatable, Identifiable {
    let option: SpeechVoiceOption
    let availability: SpeechVariantAvailability

    var id: String { option.voiceID }
    var voiceID: String { option.voiceID }
    var isAvailable: Bool {
        if case .available = availability { return true }
        return false
    }
}

/// The catalog, grouped for the two-level picker.
struct SpeechVoiceCatalog: Equatable {
    /// Grouped by `voice_name`, in the order the catalog listed them so the
    /// provider's own ordering is preserved rather than re-sorted here.
    let groups: [SpeechVoiceGroup]

    var isEmpty: Bool { groups.isEmpty }

    /// The voice the stored profile points at, when the catalog still has it.
    ///
    /// Returning nil is the important part: ADR 0007 says an unavailable
    /// default must ask the user to choose rather than falling back to the
    /// first entry in the list.
    func group(containing voiceID: String?) -> SpeechVoiceGroup? {
        guard let voiceID else { return nil }
        return groups.first { group in
            group.variants.contains { $0.voiceID == voiceID }
        }
    }
}

struct SpeechVoiceGroup: Equatable, Identifiable {
    let name: String
    let variants: [SpeechVariantOption]

    var id: String { name }

    /// A group whose every variant is unusable is not a choice, so it is
    /// reported as such rather than being offered and then failing.
    var isUsable: Bool { variants.contains(where: \.isAvailable) }
}

enum SpeechVoiceCatalogBuilder {
    /// Which providers this app presents.
    ///
    /// Not a list to choose from: ADR 0007 names SenseAudio as the first
    /// provider rather than the only one forever, and the UI states the one it
    /// has been exercised against instead of implying the others are one click
    /// away. Entries from any other provider are dropped rather than shown
    /// greyed, because the app has never verified a call through them.
    static let presentedProvider = "senseaudio"

    static func build(
        from receipt: VoiceCatalogReceipt,
        unavailableReason: String? = nil
    ) -> SpeechVoiceCatalog {
        let usable = receipt.voices.filter { $0.provider == presentedProvider }

        // Group order follows first appearance, so a voice the provider lists
        // first stays first.
        var order: [String] = []
        var variantsByName: [String: [SpeechVoiceOption]] = [:]
        for voice in usable {
            let name = Self.displayName(for: voice)
            if variantsByName[name] == nil {
                variantsByName[name] = []
                order.append(name)
            }
            variantsByName[name]?.append(Self.option(from: voice))
        }

        let groups = order.map { name -> SpeechVoiceGroup in
            let options = variantsByName[name] ?? []
            let variants = options.map { option in
                let availability: SpeechVariantAvailability = unavailableReason.map {
                    .unavailable(reason: $0)
                } ?? .available
                return SpeechVariantOption(option: option, availability: availability)
            }
            return SpeechVoiceGroup(name: name, variants: variants)
        }

        return SpeechVoiceCatalog(groups: groups)
    }

    private static func displayName(for voice: VoiceCatalogVoice) -> String {
        if let name = voice.voiceName, !name.isEmpty {
            return name
        }
        // The contract does not guarantee `voice_name`, and an unnamed row that
        // shows a raw id is still usable; a blank row is not.
        return voice.voiceID
    }

    private static func option(from voice: VoiceCatalogVoice) -> SpeechVoiceOption {
        SpeechVoiceOption(
            voiceID: voice.voiceID,
            displayName: voice.voiceName ?? voice.voiceID,
            emotionLabel: voice.emotionLabel,
            styleLabel: voice.styleLabel,
            sourceType: voice.sourceType,
            description: voice.description
        )
    }
}
