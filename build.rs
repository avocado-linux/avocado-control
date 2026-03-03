fn main() {
    varlink_generator::cargo_build_tosource(
        "src/varlink/org.avocado.Extensions.varlink",
        true,
    );
    varlink_generator::cargo_build_tosource(
        "src/varlink/org.avocado.Runtimes.varlink",
        true,
    );
    varlink_generator::cargo_build_tosource(
        "src/varlink/org.avocado.Hitl.varlink",
        true,
    );
    varlink_generator::cargo_build_tosource(
        "src/varlink/org.avocado.RootAuthority.varlink",
        true,
    );
}
