#include <gtest/gtest.h>

#include "vortia/geometry/StlReader.hpp"

#include <cstring>
#include <filesystem>
#include <fstream>

using namespace vortia;

namespace {

class StlReaderTest : public ::testing::Test {
protected:
    void SetUp() override {
        dir_ = std::filesystem::temp_directory_path() /
               ("vortia-stl-" + std::to_string(std::hash<std::string>{}(
                                    ::testing::UnitTest::GetInstance()
                                        ->current_test_info()
                                        ->name())));
        std::filesystem::remove_all(dir_);
        std::filesystem::create_directories(dir_);
    }
    void TearDown() override { std::filesystem::remove_all(dir_); }

    std::filesystem::path write(const std::string& name, const std::string& contents) {
        const auto path = dir_ / name;
        std::ofstream(path, std::ios::binary) << contents;
        return path;
    }

    std::filesystem::path dir_;
};

std::string oneTriangleAscii() {
    return R"(solid tri
  facet normal 0 0 1
    outer loop
      vertex 0 0 0
      vertex 1 0 0
      vertex 0 1 0
    endloop
  endfacet
endsolid tri
)";
}

std::string oneTriangleBinary() {
    std::string bytes(84 + 50, '\0');
    const std::uint32_t count = 1;
    std::memcpy(bytes.data() + 80, &count, sizeof(count));
    const float v[12] = {0, 0, 1,  0, 0, 0,  1, 0, 0,  0, 1, 0};
    std::memcpy(bytes.data() + 84, v, sizeof(v));
    return bytes;
}

TEST_F(StlReaderTest, ReadsAscii) {
    auto mesh = StlReader::read(write("tri_ascii.stl", oneTriangleAscii()));
    ASSERT_TRUE(mesh.hasValue()) << mesh.error().describe();
    ASSERT_EQ(mesh->triangles().size(), 1u);
    ASSERT_EQ(mesh->vertices().size(), 3u);
    EXPECT_DOUBLE_EQ(mesh->vertices()[1].x, 1.0);
    EXPECT_DOUBLE_EQ(mesh->vertices()[2].y, 1.0);
}

TEST_F(StlReaderTest, ReadsBinary) {
    auto mesh = StlReader::read(write("tri_bin.stl", oneTriangleBinary()));
    ASSERT_TRUE(mesh.hasValue()) << mesh.error().describe();
    ASSERT_EQ(mesh->triangles().size(), 1u);
    EXPECT_DOUBLE_EQ(mesh->vertices()[1].x, 1.0);
}

TEST_F(StlReaderTest, BinaryWithSolidHeaderIsStillBinary) {
    std::string bytes = oneTriangleBinary();
    // Exporter quirk: binary file whose header starts with "solid".
    std::memcpy(bytes.data(), "solid binary trick", 18);
    auto mesh = StlReader::read(write("tricky.stl", bytes));
    ASSERT_TRUE(mesh.hasValue()) << mesh.error().describe();
    EXPECT_EQ(mesh->triangles().size(), 1u);
}

TEST_F(StlReaderTest, RejectsTruncatedBinary) {
    std::string bytes = oneTriangleBinary();
    bytes.resize(bytes.size() - 10);
    auto mesh = StlReader::read(write("truncated.stl", bytes));
    EXPECT_FALSE(mesh.hasValue());
}

TEST_F(StlReaderTest, RejectsMalformedAscii) {
    auto mesh = StlReader::read(write("bad.stl", "solid x\nfacet normal 0 0\n"));
    EXPECT_FALSE(mesh.hasValue());
}

TEST_F(StlReaderTest, RejectsEmptySolid) {
    auto mesh = StlReader::read(write("empty.stl", "solid x\nendsolid x\n"));
    ASSERT_FALSE(mesh.hasValue());
    EXPECT_EQ(mesh.error().code, ErrorCode::InvalidData);
}

TEST_F(StlReaderTest, RejectsMissingFile) {
    auto mesh = StlReader::read(dir_ / "nope.stl");
    ASSERT_FALSE(mesh.hasValue());
    EXPECT_EQ(mesh.error().code, ErrorCode::NotFound);
}

TEST_F(StlReaderTest, RejectsNonFiniteVertex) {
    auto mesh = StlReader::read(write("nan.stl", R"(solid x
facet normal 0 0 1
 outer loop
  vertex 0 0 0
  vertex nan 0 0
  vertex 0 1 0
 endloop
endfacet
endsolid
)"));
    EXPECT_FALSE(mesh.hasValue());
}

} // namespace
