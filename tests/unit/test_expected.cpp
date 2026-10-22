#include <gtest/gtest.h>

#include "vortia/core/Expected.hpp"

using namespace vortia;

namespace {

Expected<int> ok() { return 42; }
Expected<int> fail() { return Error::make(ErrorCode::NotFound, "missing", "ctx"); }
Expected<void> okVoid() { return {}; }
Expected<void> failVoid() { return Error::make(ErrorCode::IoError, "disk full"); }

} // namespace

TEST(Expected, HoldsValue) {
    auto r = ok();
    ASSERT_TRUE(r.hasValue());
    EXPECT_EQ(r.value(), 42);
}

TEST(Expected, HoldsError) {
    auto r = fail();
    ASSERT_FALSE(r.hasValue());
    EXPECT_EQ(r.error().code, ErrorCode::NotFound);
    EXPECT_EQ(r.error().describe(), "missing [ctx]");
}

TEST(Expected, ValueAccessOnErrorThrows) {
    auto r = fail();
    EXPECT_THROW(r.value(), std::logic_error);
}

TEST(Expected, VoidSuccess) {
    auto r = okVoid();
    EXPECT_TRUE(r.hasValue());
    EXPECT_NO_THROW(r.value());
}

TEST(Expected, VoidError) {
    auto r = failVoid();
    EXPECT_FALSE(r.hasValue());
    EXPECT_EQ(r.error().code, ErrorCode::IoError);
}
