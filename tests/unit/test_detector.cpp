#include <gtest/gtest.h>

#include "vortia/solver/openfoam/Detector.hpp"

#include <filesystem>
#include <fstream>
#include <functional>

using namespace vortia;
using namespace vortia::openfoam;

namespace {

class DetectorTest : public ::testing::Test {
protected:
    void SetUp() override {
        dir_ = std::filesystem::temp_directory_path() /
               ("vortia-foam-" + std::to_string(std::hash<std::string>{}(
                                     ::testing::UnitTest::GetInstance()
                                         ->current_test_info()
                                         ->name())));
        std::filesystem::remove_all(dir_);
    }
    void TearDown() override { std::filesystem::remove_all(dir_); }

    /// Builds a minimal fake OpenFOAM root: <root>/etc/bashrc with a version.
    std::filesystem::path fakeRoot(const std::string& name, const std::string& versionLine) {
        const auto root = dir_ / name;
        std::filesystem::create_directories(root / "etc");
        std::ofstream(root / "etc" / "bashrc")
            << "# fake OpenFOAM bashrc for tests\n"
            << "export WM_PROJECT=OpenFOAM\n"
            << versionLine << "\n";
        return root;
    }

    std::filesystem::path dir_;
};

class ActivationRunner : public IProcessRunner {
public:
    std::vector<ProcessRequest> requests;
    std::function<ProcessResult(const ProcessRequest&)> respond;
    Expected<ProcessResult> run(const ProcessRequest& request, CancellationToken*,
                                OutputCallback) override {
        requests.push_back(request);
        return respond(request);
    }
};

TEST_F(DetectorTest, ActivatesNativeAppAndProbesMountedRoot) {
    const auto apps=dir_/"Applications with spaces";
    const auto resources=apps/"OpenFOAM-v2606.app"/"Contents"/"Resources";
    std::filesystem::create_directories(resources);
    std::ofstream(resources/"volume") << "fixture";
    std::ofstream(resources/"OpenFOAM-v2606.dmg") << "fixture";
    const auto root=dir_/"OpenFOAM-v2606";
    ActivationRunner runner;
    runner.respond=[&](const ProcessRequest& r) {
        ProcessResult result;result.started=true;result.exitCode=0;
        EXPECT_EQ(r.executable,resources/"volume");
        if(r.arguments==std::vector<std::string>{"-show-prefix"})result.stdOut=root.string()+"\n";
        else {
            EXPECT_EQ(r.arguments,(std::vector<std::string>{"-quiet","mount"}));
            EXPECT_EQ(r.timeout,std::chrono::seconds{30});
            fakeRoot("OpenFOAM-v2606","WM_PROJECT_VERSION=v2606");
        }
        return result;
    };
    auto found=Detector::discover(runner,root,{}, {apps});
    ASSERT_TRUE(found) << found.error().describe();
    ASSERT_EQ(found->size(),1U);EXPECT_EQ(found->front().rootDir,root);
    EXPECT_EQ(runner.requests.size(),2U);
    // A second request must use the mounted root without launching another helper.
    ASSERT_TRUE(Detector::discover(runner,root,{}, {apps}));
    EXPECT_EQ(runner.requests.size(),2U);
}

TEST_F(DetectorTest, NeverSubstitutesAnotherVersionForConfiguredRoot) {
    const auto apps=dir_/"Applications";
    const auto resources=apps/"OpenFOAM-v2606.app"/"Contents"/"Resources";
    std::filesystem::create_directories(resources);
    std::ofstream(resources/"volume") << "fixture";
    std::ofstream(resources/"OpenFOAM-v2606.dmg") << "fixture";
    ActivationRunner runner;
    runner.respond=[&](const ProcessRequest& r){
        EXPECT_EQ(r.arguments,(std::vector<std::string>{"-show-prefix"}));
        ProcessResult result;result.started=true;result.exitCode=0;
        result.stdOut=(dir_/"OpenFOAM-v2606").string()+"\n";return result;
    };
    const auto result=Detector::discover(runner,dir_/"OpenFOAM-14",{}, {apps});
    ASSERT_FALSE(result);EXPECT_EQ(result.error().code,ErrorCode::NotFound);
    EXPECT_EQ(runner.requests.size(),1U);
}

TEST_F(DetectorTest, RejectsInvalidNativeAppPrefixAndSurfacesMountFailures) {
    const auto apps=dir_/"Applications";
    const auto resources=apps/"OpenFOAM-v2606.app"/"Contents"/"Resources";
    std::filesystem::create_directories(resources);
    std::ofstream(resources/"volume") << "fixture";
    std::ofstream(resources/"OpenFOAM-v2606.dmg") << "fixture";
    const auto root=dir_/"OpenFOAM-v2606";
    ActivationRunner runner;
    for(const auto& bad:std::vector<std::string>{"relative",root.string()+"\nextra",(dir_/"wrong").string()}) {
        runner.requests.clear();
        runner.respond=[&](const ProcessRequest&){ProcessResult r;r.started=true;r.exitCode=0;r.stdOut=bad;return r;};
        const auto found=Detector::discover(runner,root,{}, {apps});
        ASSERT_FALSE(found);EXPECT_EQ(found.error().code,ErrorCode::InvalidData);
        EXPECT_EQ(runner.requests.size(),1U);
    }
    for(const bool timeout:{false,true}) {
        runner.respond=[&](const ProcessRequest& request){
            ProcessResult r;r.started=true;
            if(request.arguments.front()=="-show-prefix"){r.exitCode=0;r.stdOut=root.string();}
            else{r.exitCode=1;r.timedOut=timeout;r.stdErr="Mount diagnostic";}
            return r;
        };
        const auto found=Detector::discover(runner,root,{}, {apps});
        ASSERT_FALSE(found);
        EXPECT_EQ(found.error().code,timeout?ErrorCode::Timeout:ErrorCode::ProcessFailed);
        EXPECT_NE(found.error().describe().find("Mount diagnostic"),std::string::npos);
    }
    runner.respond=[&](const ProcessRequest& request){ProcessResult r;r.started=true;r.exitCode=0;
        if(request.arguments.front()=="-show-prefix")r.stdOut=root.string();return r;};
    // A successful helper exit is not enough: mounted etc/bashrc must actually exist.
    EXPECT_FALSE(Detector::discover(runner,root,{}, {apps}));
}

TEST_F(DetectorTest, IgnoresIncompleteAppAndRejectsRelativePreference) {
    const auto apps=dir_/"Applications";
    std::filesystem::create_directories(apps/"OpenFOAM-v2606.app"/"Contents"/"Resources");
    ActivationRunner runner; // no invocation is allowed
    EXPECT_FALSE(Detector::discover(runner,dir_/"OpenFOAM-v2606",{}, {apps}));
    const auto relative=Detector::discover(runner,"relative",{}, {apps});
    ASSERT_FALSE(relative);EXPECT_EQ(relative.error().code,ErrorCode::InvalidArgument);
    EXPECT_TRUE(runner.requests.empty());
}

TEST_F(DetectorTest, BundledEngineNeedsNoExternalInstallationOrActivation) {
    const auto integrated=fakeRoot("bundled/OpenFOAM-v2606","WM_PROJECT_VERSION=v2606");
    ActivationRunner runner; // any process invocation fails this test
    const auto found=Detector::discover(runner,{},integrated,{dir_/"NoApplications"});
    ASSERT_TRUE(found);ASSERT_EQ(found->size(),1U);
    EXPECT_EQ(found->front().rootDir,integrated);
    const auto chosen=fakeRoot("external/OpenFOAM-14","WM_PROJECT_VERSION=14");
    const auto explicitChoice=Detector::discover(runner,chosen,integrated);
    ASSERT_TRUE(explicitChoice);EXPECT_EQ(explicitChoice->front().rootDir,chosen);
    // A broken packaged engine must be diagnosed, not quietly use another version.
    ASSERT_TRUE(std::filesystem::remove(integrated/"etc"/"bashrc"));
    EXPECT_FALSE(Detector::discover(runner,{},integrated));
    EXPECT_TRUE(runner.requests.empty());
}

TEST_F(DetectorTest, BundledImageIsActivatedBeforeSearchingExternalInstallations) {
    const auto resources=dir_/"Vortia.app"/"Contents"/"Resources"/"OpenFOAM-v2606";
    std::filesystem::create_directories(resources);
    std::ofstream(resources/"volume")<<"fixture";
    std::ofstream(resources/"OpenFOAM-v2606.dmg")<<"fixture";
    const auto root=dir_/"Vortia-OpenFOAM-v2606";
    ActivationRunner runner;
    runner.respond=[&](const ProcessRequest& request){
        EXPECT_EQ(request.executable,resources/"volume");
        ProcessResult r;r.started=true;r.exitCode=0;
        if(request.arguments.front()=="-show-prefix")r.stdOut=root.string()+"\n";
        else fakeRoot("Vortia-OpenFOAM-v2606","WM_PROJECT_VERSION=v2606");
        return r;
    };
    const auto found=Detector::discover(runner,{},resources,{dir_/"NoExternalApps"});
    ASSERT_TRUE(found);ASSERT_EQ(found->size(),1U);EXPECT_EQ(found->front().rootDir,root);
    EXPECT_EQ(runner.requests.size(),2U);
}

TEST_F(DetectorTest, NativeInstalledEnvironmentSmoke) {
    const char* nativeRoot=std::getenv("VORTIA_NATIVE_OF_ROOT");
    if(!nativeRoot)GTEST_SKIP()<<"Set VORTIA_NATIVE_OF_ROOT for real installed-tool verification";
    PosixProcessRunner runner;
    const auto found=Detector::discover(runner,nativeRoot);
    ASSERT_TRUE(found);ASSERT_FALSE(found->empty());
    const auto env=Detector::captureEnvironment(found->front(),runner);ASSERT_TRUE(env);
    const auto caps=Detector::verifyCapabilities(found->front(),*env);
    EXPECT_TRUE(caps.hasBlockMesh);EXPECT_TRUE(caps.hasCheckMesh);EXPECT_TRUE(caps.hasFoamToVtk);
    for(const auto* name:{"blockMesh","checkMesh","rhoSimpleFoam"}) {
        const auto exe=Detector::resolveExecutable(name,*env);ASSERT_TRUE(exe);
        ProcessRequest request;request.executable=*exe;request.arguments={"-help"};
        request.environment=*env;request.timeout=std::chrono::seconds{10};
        const auto result=runner.run(request);ASSERT_TRUE(result);EXPECT_TRUE(result->succeeded());
    }
}

TEST_F(DetectorTest, ProbesEsiInstallation) {
    const auto root = fakeRoot("OpenFOAM-v2606", "export WM_PROJECT_VERSION=v2606");
    auto installation = Detector::probe(root);
    ASSERT_TRUE(installation.hasValue()) << installation.error().describe();
    EXPECT_EQ(installation->distribution, Distribution::Esi);
    EXPECT_EQ(installation->version.major, 2606);
}

TEST_F(DetectorTest, ProbesFoundationInstallation) {
    const auto root = fakeRoot("OpenFOAM-14", "WM_PROJECT_VERSION=14");
    auto installation = Detector::probe(root);
    ASSERT_TRUE(installation.hasValue());
    EXPECT_EQ(installation->distribution, Distribution::Foundation);
    EXPECT_EQ(installation->version.major, 14);
}

TEST_F(DetectorTest, RejectsDirectoryWithoutBashrc) {
    const auto root = dir_ / "not-foam";
    std::filesystem::create_directories(root);
    auto installation = Detector::probe(root);
    ASSERT_FALSE(installation.hasValue());
    EXPECT_EQ(installation.error().code, ErrorCode::NotFound);
}

TEST_F(DetectorTest, RejectsBashrcWithoutVersion) {
    const auto root = dir_ / "OpenFOAM-broken";
    std::filesystem::create_directories(root / "etc");
    std::ofstream(root / "etc" / "bashrc") << "# nothing useful here\n";
    auto installation = Detector::probe(root);
    ASSERT_FALSE(installation.hasValue());
    EXPECT_EQ(installation.error().code, ErrorCode::ParseError);
}

TEST_F(DetectorTest, ResolveExecutableSearchesPath) {
    // A fake executable inside a temp bin dir.
    const auto bin = dir_ / "bin";
    std::filesystem::create_directories(bin);
    const auto exe = bin / "blockMesh";
    {
        std::ofstream(exe) << "#!/bin/sh\nexit 0\n";
        std::filesystem::permissions(exe, std::filesystem::perms::owner_all);
    }
    std::vector<std::pair<std::string, std::string>> env = {{"PATH", bin.string()}};
    auto resolved = Detector::resolveExecutable("blockMesh", env);
    ASSERT_TRUE(resolved.hasValue());
    EXPECT_EQ(resolved.value(), exe);
    EXPECT_FALSE(Detector::resolveExecutable("missingTool", env).hasValue());
}

TEST_F(DetectorTest, ResolveExecutableRejectsPathTraversal) {
    std::vector<std::pair<std::string, std::string>> env = {{"PATH", dir_.string()}};
    EXPECT_FALSE(Detector::resolveExecutable("../evil", env).hasValue());
    EXPECT_FALSE(Detector::resolveExecutable("/abs/path", env).hasValue());
    EXPECT_FALSE(Detector::resolveExecutable("", env).hasValue());
}

} // namespace
