fn main() {
    varlink_generator::cargo_build_tosource("src/varlink/org.avocado.Extensions.varlink", false);
    varlink_generator::cargo_build_tosource("src/varlink/org.avocado.Runtimes.varlink", false);
    varlink_generator::cargo_build_tosource("src/varlink/org.avocado.Hitl.varlink", false);
    varlink_generator::cargo_build_tosource("src/varlink/org.avocado.RootAuthority.varlink", false);
}
