#include "vortia/solver/openfoam/LogParser.hpp"

#include <cmath>
#include <regex>
#include <nlohmann/json.hpp>
#include <algorithm>
#include <array>

namespace vortia::openfoam {

namespace {

// smoothSolver:  Solving for Ux, Initial residual = 1, Final residual = 2.96335e-06, No Iterations 19
const std::regex kResidual(
    R"(\bSolving for (\w+), Initial residual = ([0-9.eE+\-nanif]+), Final residual = ([0-9.eE+\-nanif]+), No Iterations (\d+))");

const std::regex kExecution(R"(ExecutionTime = ([0-9.eE+\-]+) s\s+ClockTime = ([0-9.eE+\-]+) s)");

const std::regex kTime(R"(^\s*Time = ([0-9.eE+\-]+)\s*$)");

const std::regex kCourant(R"(Courant Number mean: ([0-9.eE+\-nanif]+) max: ([0-9.eE+\-nanif]+))");
const std::regex kCentralCourant(R"(Courant Numbers: min = [0-9.eE+\-nanif]+, average = ([0-9.eE+\-nanif]+), max = ([0-9.eE+\-nanif]+))");

[[nodiscard]] bool looksFatal(const std::string& line) {
    return line.find("FOAM FATAL") != std::string::npos ||
           line.find("floating point exception", 0) != std::string::npos ||
           line.find("FOAM SIGFPE") != std::string::npos;
}

[[nodiscard]] bool isBadNumber(double v) { return std::isnan(v) || std::isinf(v); }

} // namespace

std::optional<ConvergenceSample> parseConvergenceReport(std::string_view text) {
    try {
        if(text.size()>8192)return {};
        const auto j=nlohmann::json::parse(text);
        if(j.at("schema")!=1)return {};
        ConvergenceSample s;
        s.status=j.at("status").get<std::string>();
        const std::array<std::string_view,7> states{"warming","checking","stabilizing","converged","invalid","limit","interrupted"};
        if(std::find(states.begin(),states.end(),s.status)==states.end())return {};
        s.iteration=j.at("iteration").get<double>();s.checkedIteration=j.at("checkedIteration").get<double>();s.sampled=j.at("sampled").get<bool>();
        s.automatic=j.at("automatic").get<bool>();
        s.globalMass=j.at("globalMass").get<double>();s.localMass=j.at("localMass").get<double>();
        s.fieldChange=j.at("fieldChange").get<double>();
        s.equationError=j.at("equationError").get<double>();s.equationTolerance=j.at("equationTolerance").get<double>();
        s.massTolerance=j.at("massTolerance").get<double>();s.fieldTolerance=j.at("fieldTolerance").get<double>();
        s.stableSamples=j.at("stableSamples").get<int>();s.requiredSamples=j.at("requiredSamples").get<int>();
        s.interval=j.at("interval").get<int>();
        for(double v:{s.iteration,s.checkedIteration,s.globalMass,s.localMass,s.fieldChange,s.equationError,s.equationTolerance,s.massTolerance,s.fieldTolerance})
            if(!std::isfinite(v)||v<0)return {};
        if(s.massTolerance<1e-8||s.massTolerance>.01||s.fieldTolerance<1e-8||s.fieldTolerance>.01||s.equationTolerance<1e-8||s.equationTolerance>1e-3||
           s.stableSamples<0||s.requiredSamples<2||s.requiredSamples>100||s.interval<1||s.interval>10000)return {};
        if(s.checkedIteration>s.iteration)return {};
        if(s.status=="converged"&&(!s.sampled||s.checkedIteration!=s.iteration||s.stableSamples<s.requiredSamples||
            s.globalMass>s.massTolerance||s.localMass>s.massTolerance||s.fieldChange>s.fieldTolerance||s.equationError>s.equationTolerance))return {};
        if(j.contains("massIncoming")||j.contains("massOutgoing")){
            const auto incoming=j.at("massIncoming").get<double>(),outgoing=j.at("massOutgoing").get<double>();
            if(!std::isfinite(incoming)||!std::isfinite(outgoing)||incoming<0||outgoing<0)return {};
            s.massIncoming=incoming;s.massOutgoing=outgoing;
        }
        return s;
    } catch(...) {return {};}
}

void LogParser::feed(std::string_view chunk) {
    pending_.append(chunk);
    std::size_t pos = 0;
    while ((pos = pending_.find('\n')) != std::string::npos) {
        std::string line = pending_.substr(0, pos);
        if (!line.empty() && line.back() == '\r')
            line.pop_back();
        pending_.erase(0, pos + 1);
        processLine(line);
    }
}

void LogParser::flush() {
    if (!pending_.empty()) {
        std::string line = pending_;
        if (!line.empty() && line.back() == '\r')
            line.pop_back();
        pending_.clear();
        processLine(line);
    }
}

void LogParser::reset() {
    pending_.clear();
    progress_ = SolverProgress{};monitorSamples_.clear();hasTime_=false;session_=0;historyPhase_=true;pendingCentralCourant_.reset();clockBaseline_.reset();
}

std::vector<MonitorSample> LogParser::takeMonitorSamples(){auto samples=std::move(monitorSamples_);monitorSamples_.clear();return samples;}
void LogParser::monitor(const std::string& key,double x,double value){
    if(collectMonitors_&&historyPhase_&&std::isfinite(x)&&x>=0&&std::isfinite(value)&&value>=0)monitorSamples_.push_back({x,value,key,session_});
}
void LogParser::processLine(const std::string& line) {
    if(collectMonitors_&&line.rfind("===== Vortia step:",0)==0){
        historyPhase_=line.find("Foam")!=std::string::npos||line.find("(mpirun)")!=std::string::npos;
        if(historyPhase_)++session_;hasTime_=false;clockBaseline_.reset();pendingCentralCourant_.reset();
        return;
    }
    constexpr std::string_view marker="VORTIA_CONVERGENCE ";
    if(line.rfind(marker,0)==0) {
        if(auto sample=parseConvergenceReport(std::string_view(line).substr(marker.size()))){
            if(sample->sampled&&sample->checkedIteration==sample->iteration){
                const auto x=sample->checkedIteration;
                monitor("mass.global",x,sample->globalMass);monitor("mass.local",x,sample->localMass);monitor("mass.tolerance",x,sample->massTolerance);
                monitor("field.change",x,sample->fieldChange);monitor("field.tolerance",x,sample->fieldTolerance);
                monitor("equation.error",x,sample->equationError);monitor("equation.tolerance",x,sample->equationTolerance);
                monitor("stability.samples",x,sample->stableSamples);monitor("stability.required",x,sample->requiredSamples);
                if(sample->massIncoming){monitor("flow.incoming",x,*sample->massIncoming);monitor("flow.outgoing",x,*sample->massOutgoing);}
            }
            progress_.convergence=std::move(sample);
        }
        return;
    }
    if(line.find("No positive conservative update after 30 reductions")!=std::string::npos){
        progress_.diverged=true;progress_.divergenceKind=DivergenceKind::NonPositiveUpdate;
        progress_.divergenceReason="No positive conservative update after 30 reductions";return;
    }
    if (looksFatal(line)) {
        progress_.diverged = true;
        if(progress_.divergenceKind!=DivergenceKind::NonPositiveUpdate){progress_.divergenceKind=DivergenceKind::Fatal;progress_.divergenceReason=line;}
        return;
    }

    std::smatch match;
    if (line.find("Time =")!=std::string::npos&&std::regex_search(line, match, kTime)) {
        try {
            const double coordinate=std::stod(match[1].str());
            if(collectMonitors_&&hasTime_&&coordinate<progress_.time){++session_;clockBaseline_.reset();}
            progress_.time = coordinate;hasTime_=true;
            if(pendingCentralCourant_){monitor("courant.mean",coordinate,pendingCentralCourant_->first);monitor("courant.max",coordinate,pendingCentralCourant_->second);pendingCentralCourant_.reset();}
        } catch (...) {
        }
        return;
    }
    if (line.find("Courant Number")!=std::string::npos&&(std::regex_search(line, match, kCourant) || std::regex_search(line, match, kCentralCourant))) {
        try {
            progress_.courantMean = std::stod(match[1].str());
            progress_.courantMax = std::stod(match[2].str());
        } catch (...) {
        }
        if(line.find("Courant Numbers:")!=std::string::npos)pendingCentralCourant_={{progress_.courantMean,progress_.courantMax}};
        else if(hasTime_){monitor("courant.mean",progress_.time,progress_.courantMean);monitor("courant.max",progress_.time,progress_.courantMax);}
        if (isBadNumber(progress_.courantMax)) {
            progress_.diverged = true;
            if(progress_.divergenceKind!=DivergenceKind::NonPositiveUpdate){progress_.divergenceKind=DivergenceKind::NonFinite;progress_.divergenceReason = "Courant number became NaN/Inf";}
        }
        return;
    }
    if(collectMonitors_&&hasTime_&&line.find("ExecutionTime =")!=std::string::npos&&std::regex_search(line,match,kExecution)){
        try {const double cpu=std::stod(match[1].str()),clock=std::stod(match[2].str());
            monitor("runtime.cpu",progress_.time,cpu);monitor("runtime.clock",progress_.time,clock);
            if(clockBaseline_&&clock>clockBaseline_->second&&progress_.time>clockBaseline_->first){monitor("runtime.rate",progress_.time,(progress_.time-clockBaseline_->first)/(clock-clockBaseline_->second));clockBaseline_={{progress_.time,clock}};}
            else if(!clockBaseline_||clock<clockBaseline_->second)clockBaseline_={{progress_.time,clock}};
        }catch(...){}
        return;
    }
    if (line.find("Solving for ")!=std::string::npos&&std::regex_search(line, match, kResidual)) {
        ResidualSample sample;
        sample.field = match[1].str();
        try {
            sample.initial = std::stod(match[2].str());
            sample.final = std::stod(match[3].str());
            sample.iterations = std::stoi(match[4].str());
        } catch (...) {
            return;
        }
        if (isBadNumber(sample.initial) || isBadNumber(sample.final)) {
            progress_.diverged = true;
            if(progress_.divergenceKind!=DivergenceKind::NonPositiveUpdate){progress_.divergenceKind=DivergenceKind::NonFinite;progress_.divergenceReason = "NaN/Inf residual for field " + sample.field;}
        }
        if(hasTime_){monitor("residual.initial."+sample.field,progress_.time,sample.initial);monitor("residual.final."+sample.field,progress_.time,sample.final);}
        for (auto& existing : progress_.residuals) {
            if (existing.field == sample.field) {
                existing = sample;
                return;
            }
        }
        progress_.residuals.push_back(std::move(sample));
    }
}

} // namespace vortia::openfoam
