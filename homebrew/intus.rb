class Intus < Formula
  desc "Local Autonomous Agent and System Sidecar for your terminal"
  homepage "https://github.com/harryw1/intus"
  url "https://github.com/harryw1/intus/archive/refs/tags/v1.1.4.tar.gz"
  sha256 "ebb4a3ea1fc78a26dcca77f7dced10b745698cf9ff28fb1ac146124d7726f381"
  license "MIT"
  head "https://github.com/harryw1/intus.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "intus", shell_output("#{bin}/intus --version")
  end
end
