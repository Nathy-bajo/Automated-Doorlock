fn main() {
    let prisma_schema = "src/prisma/schema.prisma";
    prisma_codegen::generate_prisma(&prisma_schema);
    println!("cargo:rerun-if-changed={}", prisma_schema);
}