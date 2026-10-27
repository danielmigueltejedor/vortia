#include "vortia/solver/openfoam/Detector.hpp"

#include "vortia/core/Logger.hpp"

#include <cstdlib>
#include <fstream>
#include <optional>
#include <regex>
#include <set>
#include <sstream>

namespace vortia::openfoam {

namespace {

constexpr std::size_t kMaxBashrcBytes = 4 * 1024 * 1024; // sanity limit for parsing

[[nodiscard]] Expected<std::string> readTextFile(const std::filesystem::path& path,
                                                 std::size_t maxBytes) {
    std::error_code ec;
    const auto size = std::filesystem::file_size(path, ec);
    if (ec || size > maxBytes)
        return Error::make(ErrorCode::InvalidData, "bashrc missing or too large", path.string());
    std::ifstream in(path);
    if (!in)
        return Error::make(ErrorCode::PermissionDenied, "cannot read bashrc", path.string());
    std::string text(size, '\0');
    in.read(text.data(), static_cast<std::streamsize>(size));
    text.resize(static_cast<std::size_t>(in.gcount()));
    return text;
}

/// Extracts `WM_PROJECT_VERSION=<value>` (with or without `export`, quotes
/// tolerated) from bashrc text.
[[nodiscard]] Expected<std::string> extractVersion(const std::string& bashrcText) {
    static const std::regex pattern(R"(WM_PROJECT_VERSION=([A-Za-z0-9:._+\-']+))");
    std::smatch match;
    if (!std::regex_search(bashrcText, match, pattern))
        return Error::make(ErrorCode::ParseError, "WM_PROJECT_VERSION not found in bashrc");
    std::string value = match[1].str();
    // Strip surrounding quotes if present.
    if (value.size() >= 2 && (value.front() == '"' || value.front() == '\'') &&
        value.back() == value.front())
        value = value.substr(1, value.size() - 2);
    return value;
}

[[nodiscard]] std::vector<std::filesystem::path> defaultSearchRoots() {
    std::vector<std::filesystem::path> roots;
    if (const char* wm = std::getenv("WM_PROJECT_DIR"))
        roots.emplace_back(wm);
    if (const char* vo = std::getenv("VORTIA_OPENFOAM_DIR"))
        roots.emplace_back(vo);
    roots.emplace_back("/opt");
    roots.emplace_back("/usr/lib/openfoam");
    roots.emplace_back("/usr/local/opt");
    roots.emplace_back("/opt/homebrew/opt");
    roots.emplace_back("/Volumes");
    roots.emplace_back("/Applications");
    if (const char* home = std::getenv("HOME"))
        roots.emplace_back(std::filesystem::path(home) / "OpenFOAM");
    return roots;
}

/// Heuristic: directory names that look like OpenFOAM roots.
[[nodiscard]] bool nameLooksLikeOpenFoam(const std::filesystem::path& p) {
    const std::string n = p.filename().string();
    return n.rfind("OpenFOAM", 0) == 0 || n.rfind("openfoam", 0) == 0;
}

Expected<Installation> activateResources(const std::filesystem::path& resources,
    const std::string& name, IProcessRunner& runner, const std::filesystem::path& preferredRoot) {
    const auto helper=resources/"volume";
    std::error_code ec;
    if(!std::filesystem::is_regular_file(helper,ec) ||
       !std::filesystem::is_regular_file(resources/(name+".dmg"),ec))
        return Error::make(ErrorCode::NotFound,"packaged engine files are missing",resources.string());
    ProcessRequest request;
    request.executable = helper;
    request.arguments = {"-show-prefix"};
    request.timeout = std::chrono::seconds{5};
    auto prefixResult = runner.run(request);
    if (!prefixResult || !prefixResult->succeeded()) {
        return prefixResult ? Error::make(ErrorCode::ProcessFailed,
            "could not query the OpenFOAM app volume", prefixResult->stdErr)
            : prefixResult.error();
    }
    auto prefix = prefixResult->stdOut;
    while (!prefix.empty() && (prefix.back() == '\n' || prefix.back() == '\r'))
        prefix.pop_back();
    const std::filesystem::path root(prefix);
    if (prefix.size() > 8192 || prefix.find_first_of("\r\n") != std::string::npos ||
        prefix.find('\0') != std::string::npos || !root.is_absolute() ||
        (root.filename() != name && root.filename() != "Vortia-"+name)) {
        return Error::make(ErrorCode::InvalidData,
            "OpenFOAM app returned an invalid volume path", resources.string());
    }
    if (!preferredRoot.empty() &&
        preferredRoot.lexically_normal() != root.lexically_normal())
        return Error::make(ErrorCode::NotFound,"configured root differs from engine volume",root.string());
    // Already mounted while discovery was in progress: no mount is needed.
    if (auto installed = Detector::probe(root))
        return *installed;
    request.arguments = {"-quiet", "mount"};
    request.timeout = std::chrono::seconds{30};
    const auto mounted = runner.run(request);
    if (!mounted || !mounted->succeeded())
        return mounted ? Error::make(mounted->timedOut ? ErrorCode::Timeout : ErrorCode::ProcessFailed,
            "could not activate the packaged OpenFOAM engine", mounted->stdErr) : mounted.error();
    auto installed = Detector::probe(root);
    if (!installed) return installed.error();
    log(LogLevel::Info, "Activated OpenFOAM runtime " + resources.string());
    return *installed;
}

} // namespace

Expected<Installation> Detector::probe(const std::filesystem::path& rootDir) {
    std::error_code ec;
    if (!std::filesystem::is_directory(rootDir, ec))
        return Error::make(ErrorCode::NotFound, "OpenFOAM root is not a directory",
                           rootDir.string());

    const std::filesystem::path bashrc = rootDir / "etc" / "bashrc";
    if (!std::filesystem::is_regular_file(bashrc, ec))
        return Error::make(ErrorCode::NotFound, "etc/bashrc not found under root",
                           bashrc.string());

    auto text = readTextFile(bashrc, kMaxBashrcBytes);
    if (!text)
        return text.error();
    auto versionText = extractVersion(text.value());
    if (!versionText)
        return versionText.error();
    auto version = Version::parse(versionText.value());
    if (!version)
        return version.error();

    Installation installation;
    installation.rootDir = std::filesystem::absolute(rootDir, ec);
    if (ec)
        return Error::make(ErrorCode::IoError, "cannot resolve OpenFOAM root", ec.message());
    installation.bashrc = bashrc;
    installation.version = version.value();
    // Convention (research.md §1): ESI versions carry a leading 'v' (v2606),
    // Foundation versions are bare numbers (14, 13, 10…).
    installation.distribution = (!version.value().raw.empty() &&
                                 (version.value().raw.front() == 'v' ||
                                  version.value().raw.front() == 'V'))
                                    ? Distribution::Esi
                                    : Distribution::Foundation;
    return installation;
}

std::vector<Installation> Detector::scan() {
    std::vector<Installation> found;
    std::set<std::string> seen;
    for (const auto& root : defaultSearchRoots()) {
        std::error_code ec;
        if (!std::filesystem::is_directory(root, ec))
            continue;
        // Probe the root itself (WM_PROJECT_DIR case) and one level deep.
        auto tryProbe = [&](const std::filesystem::path& candidate) {
            auto installation = probe(candidate);
            if (installation && seen.insert(installation->rootDir.string()).second)
                found.push_back(std::move(installation.value()));
        };
        tryProbe(root);
        for (const auto& entry : std::filesystem::directory_iterator(root, ec)) {
            if (ec)
                break;
            if (!entry.is_directory())
                continue;
            if (nameLooksLikeOpenFoam(entry.path()))
                tryProbe(entry.path());
        }
    }
    log(LogLevel::Info, "OpenFOAM scan found " + std::to_string(found.size()) + " installation(s)");
    return found;
}

Expected<std::vector<Installation>> Detector::discover(
    IProcessRunner& runner, const std::filesystem::path& preferredRoot,
    const std::filesystem::path& bundledRoot,
    const std::vector<std::filesystem::path>& applicationDirs) {
    if (!preferredRoot.empty()) {
        if (!preferredRoot.is_absolute())
            return Error::make(ErrorCode::InvalidArgument, "OpenFOAM root must be absolute",
                               preferredRoot.string());
        if (auto installed = probe(preferredRoot))
            return std::vector<Installation>{*installed};
    }
    std::error_code bundleError;
    if (!bundledRoot.empty() && std::filesystem::exists(bundledRoot,bundleError)) {
        if(preferredRoot.empty()) {
            if(auto installed=probe(bundledRoot))return std::vector<Installation>{*installed};
        }
        const auto activated=activateResources(bundledRoot,"OpenFOAM-v2606",runner,preferredRoot);
        if(activated)return std::vector<Installation>{*activated};
        if(preferredRoot.empty())return activated.error();
    }
    if(preferredRoot.empty()) {
        if(auto installed=scan();!installed.empty())return installed;
    }

    auto directories = applicationDirs;
#ifdef __APPLE__
    if (directories.empty()) {
        directories.emplace_back("/Applications");
        if (const char* home = std::getenv("HOME"))
            directories.emplace_back(std::filesystem::path(home) / "Applications");
    }
#endif
    std::optional<Error> lastError;
    for (const auto& directory : directories) {
        std::error_code ec;
        const std::filesystem::directory_iterator end;
        for (std::filesystem::directory_iterator it(directory, ec); !ec && it != end;
             it.increment(ec)) {
            const auto bundle = it->path();
            if (!bundle.is_absolute() || bundle.extension() != ".app" ||
                !nameLooksLikeOpenFoam(bundle))
                continue;
            const auto name = bundle.stem().string();
            const auto resources = bundle / "Contents" / "Resources";
            const auto helper = resources / "volume";
            std::error_code fileError;
            if (!std::filesystem::is_regular_file(helper, fileError) ||
                !std::filesystem::is_regular_file(resources / (name + ".dmg"), fileError))
                continue;
            auto installed=activateResources(resources,name,runner,preferredRoot);
            if(installed)return std::vector<Installation>{*installed};
            if(installed.error().code!=ErrorCode::NotFound)lastError=installed.error();

        }
    }
    if (lastError) return *lastError;
    if (!preferredRoot.empty())
        return Error::make(ErrorCode::NotFound,
            "configured OpenFOAM installation is unavailable", preferredRoot.string());
    return std::vector<Installation>{};
}

Expected<std::vector<std::pair<std::string, std::string>>>
Detector::captureEnvironment(const Installation& installation, IProcessRunner& runner) {
    // The path is validated before it ever reaches a shell: absolute, no
    // single quote (would break quoting), must be a regular file.
    const std::string bashrc = installation.bashrc.string();
    if (!installation.bashrc.is_absolute())
        return Error::make(ErrorCode::InvalidArgument, "bashrc path must be absolute");
    if (bashrc.find('\'') != std::string::npos)
        return Error::make(ErrorCode::InvalidArgument,
                           "bashrc path contains a single quote; refusing shell invocation",
                           bashrc);
    std::error_code ec;
    if (!std::filesystem::is_regular_file(installation.bashrc, ec))
        return Error::make(ErrorCode::NotFound, "bashrc not found", bashrc);

    ProcessRequest request;
    request.executable = "/bin/bash";
    request.arguments = {"--noprofile", "--norc", "-c",
                         "source '" + bashrc + "' >/dev/null 2>&1 && env"};
    request.timeout = std::chrono::milliseconds{15'000};

    auto result = runner.run(request);
    if (!result)
        return result.error();
    if (!result->succeeded())
        return Error::make(ErrorCode::ProcessFailed,
                           "failed to source OpenFOAM environment",
                           result->stdErr);

    std::vector<std::pair<std::string, std::string>> env;
    std::istringstream lines(result->stdOut);
    std::string line;
    while (std::getline(lines, line)) {
        if (!line.empty() && line.back() == '\r')
            line.pop_back();
        const auto eq = line.find('=');
        if (eq != std::string::npos && eq > 0)
            env.emplace_back(line.substr(0, eq), line.substr(eq + 1));
    }
    if (env.empty())
        return Error::make(ErrorCode::ProcessFailed,
                           "sourcing bashrc produced an empty environment", bashrc);
    return env;
}

Expected<std::filesystem::path>
Detector::resolveExecutable(const std::string& name,
                            const std::vector<std::pair<std::string, std::string>>& environment) {
    if (name.empty() || name.find('/') != std::string::npos ||
        name.find("..") != std::string::npos)
        return Error::make(ErrorCode::InvalidArgument,
                           "executable name must be a plain file name", name);
    std::string pathValue;
    for (const auto& [k, v] : environment)
        if (k == "PATH")
            pathValue = v;
    if (pathValue.empty())
        return Error::make(ErrorCode::NotFound, "captured environment has no PATH");

#ifdef _WIN32
    constexpr char sep = ';';
#else
    constexpr char sep = ':';
#endif
    std::istringstream dirs(pathValue);
    std::string dir;
    while (std::getline(dirs, dir, sep)) {
        if (dir.empty())
            continue;
        const std::filesystem::path candidate = std::filesystem::path(dir) / name;
        std::error_code ec;
        if (std::filesystem::is_regular_file(candidate, ec))
            return candidate;
    }
    return Error::make(ErrorCode::NotFound, "executable '" + name + "' not found in OpenFOAM PATH");
}

Capabilities Detector::verifyCapabilities(
    const Installation& installation,
    const std::vector<std::pair<std::string, std::string>>& environment) {
    Capabilities caps = capabilitiesFor(installation);
    auto present = [&](const std::string& exe) {
        return resolveExecutable(exe, environment).hasValue();
    };
    caps.hasBlockMesh = present("blockMesh");
    caps.hasSnappyHexMesh = present("snappyHexMesh");
    caps.hasCheckMesh = present("checkMesh");
    caps.hasFoamToVtk = present("foamToVTK");
    caps.hasIcoFoam = present("icoFoam");
    caps.hasSimpleFoam = present("simpleFoam");
    caps.hasFoamRun = present("foamRun");
    caps.verifiedOnSystem = caps.hasBlockMesh;
    if (!caps.verifiedOnSystem)
        log(LogLevel::Warning,
            "OpenFOAM at " + installation.rootDir.string() +
                " has no blockMesh in PATH; capabilities unverified");
    return caps;
}

} // namespace vortia::openfoam
