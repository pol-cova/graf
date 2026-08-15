cask "graf" do
  version "0.1.0"
  sha256 :no_check # Replace with release SHA256 in automated CI release workflow

  url "https://github.com/graf-editor/graf/releases/download/v#{version}/Graf-v#{version}-aarch64.dmg"
  name "Graf"
  desc "Fast, native workspace for technical writing with LaTeX, Typst, vector canvas, and AI"
  homepage "https://github.com/graf-editor/graf"

  depends_on arch: :arm64
  depends_on macos: ">= :monterey"

  app "Graf.app"

  zap trash: [
    "~/.config/graf",
    "~/Library/Application Support/Graf",
    "~/Library/Saved Application State/com.graf.editor.savedState",
  ]
end
