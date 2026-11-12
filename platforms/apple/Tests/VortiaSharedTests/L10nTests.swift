import Testing

@testable import VortiaShared

@Suite("L10n")
struct L10nTests {
    /// Names and symbols, the same in both languages.
    private static let sameInBoth: Set<L10n.Key> = [
        .appName, .edition, .laminar, .kOmegaSST, .reynolds, .courant, .openFoam,
    ]

    @Test("every key has English words")
    func english() {
        for key in L10n.Key.allCases {
            #expect(L10n.t(key, languageCode: "en") != key.rawValue, "\(key)")
        }
    }

    @Test("every key is translated to Spanish")
    func spanish() {
        for key in L10n.Key.allCases where !Self.sameInBoth.contains(key) {
            #expect(L10n.t(key, languageCode: "es") != L10n.t(key, languageCode: "en"), "\(key)")
        }
    }
}
