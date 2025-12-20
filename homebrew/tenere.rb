class Tenere < Formula
  desc "Local Autonomous Agent and System Sidecar for your terminal"
  homepage "https://github.com/harryw1/tenere"
  url "https://github.com/harryw1/tenere/archive/refs/tags/v1.0.0.tar.gz" # Update this after tagging
  sha256 "faa81656d8cf80097e1328c984b2693e5896799c9f72409bf5083a08b156b334" # Update this after tagging
  license "MIT"
  head "https://github.com/harryw1/tenere.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "tenere", shell_output("#{bin}/tenere --version")
  end
end
