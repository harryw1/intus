class Intus < Formula
  desc "Local Autonomous Agent and System Sidecar for your terminal"
  homepage "https://github.com/harryw1/intus"
  url "https://github.com/harryw1/intus/archive/refs/tags/v1.1.6.tar.gz"
  sha256 "1fe1de50e55342151c54e5543f98aff597962fd76222390053fee089718cdd60"
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
