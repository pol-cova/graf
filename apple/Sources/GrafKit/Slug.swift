import Foundation

/// Turns a project name into a folder name: "Reading and Recall!" becomes
/// "reading-and-recall". Letters from any script are kept; everything else
/// collapses to single hyphens.
public func folderSlug(for name: String) -> String {
    let folded = name.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil).lowercased()
    var slug = ""
    var pendingHyphen = false
    for scalar in folded.unicodeScalars {
        if CharacterSet.alphanumerics.contains(scalar) {
            if pendingHyphen && !slug.isEmpty { slug.append("-") }
            slug.unicodeScalars.append(scalar)
            pendingHyphen = false
        } else {
            pendingHyphen = true
        }
    }
    return slug.isEmpty ? "untitled" : slug
}
