cask "graf" do
  version "1.0.0-alpha"
  sha256 :no_check

  url "https://github.com/pol-cova/graf/releases/download/v#{version}/graf-v#{version}-arm64.dmg"
  name "graf"
  desc "Native editor for LaTeX and Typst"
  homepage "https://github.com/pol-cova/graf"

  depends_on arch: :arm64
  depends_on macos: ">= :monterey"

  app "graf.app"

  zap trash: [
    "~/.config/graf",
    "~/Library/Application Support/graf",
    "~/Library/Saved Application State/com.graf.editor.savedState",
  ]
end
