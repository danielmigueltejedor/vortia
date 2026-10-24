#include "vortia/solver/openfoam/MassBalance.hpp"
#include <algorithm>
#include <array>
#include <cmath>
#include <fstream>
#include <map>
#include <sstream>
#include <stdexcept>
#include <vector>
namespace vortia::openfoam {
namespace {
using Series=std::map<double,double>;
double number(const std::string& text) {
    std::size_t used=0;const double value=std::stod(text,&used);
    if(used!=text.size()||!std::isfinite(value))throw std::runtime_error("Invalid monitor value");
    return value;
}
Series readMonitor(const std::filesystem::path& root) {
    std::vector<std::pair<double,std::filesystem::path>> folders;
    for(const auto& entry:std::filesystem::directory_iterator(root)) {
        if(!entry.is_directory())continue;
        try {const auto start=number(entry.path().filename().string());if(start>=0)folders.emplace_back(start,entry.path());}
        catch(const std::exception&) {}
    }
    std::sort(folders.begin(),folders.end());Series result;
    for(const auto& [start,folder]:folders) {
        std::ifstream file(folder/"surfaceFieldValue.dat");if(!file)throw std::runtime_error("Missing monitor file");
        result.erase(result.lower_bound(start),result.end());
        std::string line;
        while(std::getline(file,line)) {
            std::istringstream row(line);std::string time,flow,extra;
            if(!(row>>time)||time.front()=='#')continue;
            if(!(row>>flow)||(row>>extra))throw std::runtime_error("Malformed boundary monitor row");
            const auto t=number(time),value=number(flow);
            if(t<start)throw std::runtime_error("Monitor precedes its restart");
            result[t]=value;
        }
        if(file.bad())throw std::runtime_error("Cannot read boundary monitor");
    }
    if(result.empty())throw std::runtime_error("No boundary monitor samples");
    return result;
}
}
bool isNozzlePracticeCase(const std::filesystem::path& root) {
    std::ifstream marker(root/".vortia-nozzle-case");std::string kind;std::getline(marker,kind);
    return kind=="vortia-nozzle-video-2026-v1"||kind=="vortia-nozzle-video-2026-v2"||kind=="vortia-nozzle-video-2026-v3";
}
MassBalanceResult nozzleMassBalance(const std::filesystem::path& root,double window) {
    MassBalanceResult report;
    try {
        if(!std::isfinite(window)||window<=0)throw std::runtime_error("Invalid monitor window");
        const auto post=root/"postProcessing";
        const std::array<Series,3> data{readMonitor(post/"mass"),readMonitor(post/"massOut"),readMonitor(post/"massFar")};
        const double last=data[0].rbegin()->first;report.lastIteration=last;
        for(const auto& series:data)if(series.rbegin()->first!=last)throw std::runtime_error("Boundary monitor times are not aligned");
        double sum=0;
        for(const auto& [time,inlet]:data[0]) {
            if(time<last-window)continue;
            const auto out=data[1].find(time),far=data[2].find(time);
            if(out==data[1].end()||far==data[2].end())throw std::runtime_error("Boundary monitor samples are not aligned");
            if(inlet>=-1e-15)throw std::runtime_error("Principal inlet flow is zero or reversed");
            const auto error=100*std::abs(inlet+out->second+far->second)/std::abs(inlet);
            if(!std::isfinite(error))throw std::runtime_error("Non-finite mass imbalance");
            if(report.samples==0)report.firstIteration=time;
            ++report.samples;sum+=error;
            report.maximumAbsolutePercent=std::max(report.maximumAbsolutePercent,error);
        }
        if(report.samples<3||last-report.firstIteration<0.9*window)throw std::runtime_error("Insufficient boundary monitor window");
        report.meanAbsolutePercent=sum/report.samples;report.available=true;
    } catch(const std::exception& error) {report.reason=error.what();}
    return report;
}
}
