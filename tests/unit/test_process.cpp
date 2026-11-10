#include <gtest/gtest.h>

#include "vortia/core/Process.hpp"
#include "PrivateTempDir.hpp"

#include <chrono>
#include <thread>
#include <fstream>
#include <filesystem>

using namespace vortia;
using namespace std::chrono_literals;

// POSIX-only suite (the PosixProcessRunner is a stub on Windows).
#ifndef _WIN32

TEST(Process, CapturesStdoutAndExitCode) {
    PosixProcessRunner runner;
    ProcessRequest req;
    req.executable = "/bin/sh";
    req.arguments = {"-c", "printf hello"};
    auto result = runner.run(req);
    ASSERT_TRUE(result.hasValue());
    EXPECT_TRUE(result->succeeded());
    EXPECT_EQ(result->exitCode, 0);
    EXPECT_EQ(result->stdOut, "hello");
}

TEST(Process, CapturesStderrSeparately) {
    PosixProcessRunner runner;
    ProcessRequest req;
    req.executable = "/bin/sh";
    req.arguments = {"-c", "echo out; echo err 1>&2; exit 3"};
    auto result = runner.run(req);
    ASSERT_TRUE(result.hasValue());
    EXPECT_FALSE(result->succeeded());
    EXPECT_EQ(result->exitCode, 3);
    EXPECT_EQ(result->stdOut, "out\n");
    EXPECT_EQ(result->stdErr, "err\n");
}

TEST(Process, TimeoutKillsChild) {
    PosixProcessRunner runner;
    ProcessRequest req;
    req.executable = "/bin/sleep";
    req.arguments = {"30"};
    req.timeout = 300ms;
    auto result = runner.run(req);
    ASSERT_TRUE(result.hasValue());
    EXPECT_TRUE(result->timedOut);
    EXPECT_LT(result->duration, 15s);
}

TEST(Process, CancellationStopsChild) {
    PosixProcessRunner runner;
    CancellationToken token;
    ProcessRequest req;
    req.executable = "/bin/sleep";
    req.arguments = {"30"};
    std::thread canceller([&token] {
        std::this_thread::sleep_for(200ms);
        token.cancel();
    });
    auto result = runner.run(req, &token);
    canceller.join();
    ASSERT_TRUE(result.hasValue());
    EXPECT_TRUE(result->cancelled);
    EXPECT_LT(result->duration, 15s);
}

TEST(Process, MissingExecutableReportsNotStarted) {
    PosixProcessRunner runner;
    ProcessRequest req;
    req.executable = "/definitely/not/a/real/binary";
    auto result = runner.run(req);
    ASSERT_TRUE(result.hasValue());
    EXPECT_FALSE(result->started);
}

TEST(Process, CancellationStopsDescendantsInTheOwnedProcessGroup) {
    const auto dir=vortia::test::makePrivateTempDir("vortia-cancel-");
    const auto marker=dir/"marker";
    PosixProcessRunner runner;CancellationToken token;ProcessRequest req;
    req.executable="/bin/sh";
    req.arguments={"-c","(/bin/sleep 1; echo leaked > \"$1\") & wait","vortia-group-test",marker.string()};
    std::thread canceller([&token]{std::this_thread::sleep_for(200ms);token.cancel();});
    const auto result=runner.run(req,&token);canceller.join();
    ASSERT_TRUE(result);EXPECT_TRUE(result->cancelled);
    std::this_thread::sleep_for(1100ms);
    EXPECT_FALSE(std::filesystem::exists(marker));
    std::filesystem::remove_all(dir);
}

TEST(Process, EnvironmentOverlay) {
    PosixProcessRunner runner;
    ProcessRequest req;
    req.executable = "/bin/sh";
    req.arguments = {"-c", "printf %s $VORTIA_TEST_VAR"};
    req.environment = {{"VORTIA_TEST_VAR", "present"}};
    auto result = runner.run(req);
    ASSERT_TRUE(result.hasValue());
    EXPECT_EQ(result->stdOut, "present");
}

// OpenFOAM trusts $PWD and aborts when it contains a blank; a stale value
// (launch directory, or the one captured with the engine environment) must
// never reach a child started in another directory.
TEST(Process, ChildSeesItsWorkingDirectoryAsPwd) {
    const auto dir = vortia::test::makePrivateTempDir("vortia-pwd-");
    PosixProcessRunner runner;
    ProcessRequest request;
    request.executable = "/bin/sh";
    request.arguments = {"-c", "printf '%s|%s' \"$PWD\" \"${OLDPWD-unset}\""};
    request.workingDirectory = dir;
    request.environment = {{"PWD", "/stale/Mis documentos"}, {"OLDPWD", "/stale/old"}};
    const auto result = runner.run(request);
    ASSERT_TRUE(result.hasValue());
    ASSERT_TRUE(result->succeeded());
    EXPECT_EQ(result->stdOut, std::filesystem::absolute(dir).string() + "|unset");
    std::filesystem::remove_all(dir);
}

TEST(Process, PwdIsLeftAloneWithoutAWorkingDirectory) {
    PosixProcessRunner runner;
    ProcessRequest request;
    request.executable = "/bin/sh";
    request.arguments = {"-c", "printf '%s' \"$VORTIA_TEST_MARK\""};
    request.environment = {{"VORTIA_TEST_MARK", "kept"}};
    const auto result = runner.run(request);
    ASSERT_TRUE(result.hasValue());
    EXPECT_EQ(result->stdOut, "kept");
}

TEST(Process, StreamingCallbackSeesOutput) {
    PosixProcessRunner runner;
    ProcessRequest req;
    req.executable = "/bin/sh";
    req.arguments = {"-c", "echo streamed"};
    std::string collected;
    auto result = runner.run(req, nullptr,
                             [&](OutputStream, std::string_view chunk) { collected += chunk; });
    ASSERT_TRUE(result.hasValue());
    EXPECT_EQ(collected, "streamed\n");
}

TEST(Process, RejectsNulBytesInArguments) {
    PosixProcessRunner runner;
    ProcessRequest req;
    req.executable = "/bin/echo";
    req.arguments = {std::string("bad\0arg", 7)};
    auto result = runner.run(req);
    ASSERT_FALSE(result.hasValue());
    EXPECT_EQ(result.error().code, ErrorCode::InvalidArgument);
}

#endif // !_WIN32
