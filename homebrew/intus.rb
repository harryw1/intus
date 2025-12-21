class Intus < Formula
  desc "Local Autonomous Agent and System Sidecar for your terminal"
  homepage "https://github.com/harryw1/intus"
  url "https://github.com/harryw1/intus/archive/refs/tags/v1.1.0.tar.gz"
  sha256 "1f911f079619589023772408717aeb65840e5914eb64c8431f1907444ed816c4"
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
