#include <gtest/gtest.h>

#include "PrivateTempDir.hpp"

#include <chrono>
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <string>

namespace {

class ScopedScratchWorkingDirectory {
public:
    ScopedScratchWorkingDirectory()
        : original_(std::filesystem::current_path()),
          scratch_(original_ /
                   ("private-temp-dir-test-" +
                    std::to_string(std::chrono::steady_clock::now()
                                       .time_since_epoch()
                                       .count()))) {
        if (!std::filesystem::create_directory(scratch_))
            throw std::runtime_error("failed to create scratch working directory");
        std::filesystem::current_path(scratch_);
    }

    ~ScopedScratchWorkingDirectory() {
        std::error_code ec;
        std::filesystem::current_path(original_, ec);
        std::filesystem::remove_all(scratch_, ec);
    }

private:
    std::filesystem::path original_;
    std::filesystem::path scratch_;
};

} // namespace

TEST(PrivateTempDir, CreatesUniqueDirectoriesAndRejectsFileBase) {
    ScopedScratchWorkingDirectory scratch;
    const auto base = std::filesystem::current_path() / ".vortia-test-tmp";

    const auto first = vortia::test::makePrivateTempDir("first-");
    ASSERT_EQ(first.parent_path(), base);
    ASSERT_TRUE(std::filesystem::is_directory(first));

    const auto second = vortia::test::makePrivateTempDir("second-");
    EXPECT_NE(first, second);
    EXPECT_TRUE(std::filesystem::is_directory(second));

    std::filesystem::remove_all(base);
    std::ofstream(base).put('x');
    ASSERT_TRUE(std::filesystem::is_regular_file(base));
    EXPECT_THROW(vortia::test::makePrivateTempDir("failure-"), std::runtime_error);
}
