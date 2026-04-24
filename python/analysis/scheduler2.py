import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np
import matplotlib

# Force headless mode
matplotlib.use("Agg")


def advanced_fuzzer_analysis(strategy_file, scheduler_file):
    # --- 1. Load and Prep Data ---
    df_strat = pd.read_csv(strategy_file).sort_values(by=["tick"]).ffill().fillna(0)
    df_sched = pd.read_csv(scheduler_file).sort_values(by=["tick"]).ffill().fillna(0)

    max_tick = df_strat["tick"].max()

    # Get the final state (last tick) for strategies and corpus entries
    # Since metrics like 'attempts' and 'cov_increase' are cumulative, the last row has the totals.
    final_strat = df_strat[df_strat["tick"] == max_tick].copy()
    final_sched = df_sched[df_sched["tick"] == df_sched["tick"].max()].copy()

    # Calculate Entry Age (First tick they appeared)
    entry_intro = df_sched.groupby("name")["tick"].min().rename("intro_tick")
    final_sched = final_sched.merge(entry_intro, on="name")

    # Calculate Rates safely
    final_strat["yield_rate"] = np.where(
        final_strat["attempts"] > 0, final_strat["cov_increase"] / final_strat["attempts"], 0
    )
    final_strat["syntax_err_rate"] = np.where(
        final_strat["attempts"] > 0, final_strat["syntax_err"] / final_strat["attempts"], 0
    )
    final_strat["accept_rate"] = np.where(
        final_strat["attempts"] > 0, final_strat["accepted"] / final_strat["attempts"], 0
    )

    # --- 2. Strategy Convergence & Efficiency Dashboard ---
    fig, axes = plt.subplots(2, 2, figsize=(16, 12))
    fig.suptitle("Strategy Effectiveness & Convergence Analysis", fontsize=18)

    # Plot A: Final Probability (Did they converge?)
    sns.barplot(
        data=final_strat.sort_values("probability", ascending=False),
        x="probability",
        y="name",
        ax=axes[0, 0],
        palette="viridis",
    )
    axes[0, 0].set_title("Final Assigned Probability (Convergence State)")
    axes[0, 0].set_xlabel("Probability")

    # Plot B: Coverage Yield per Attempt (Are they actually useful?)
    sns.barplot(
        data=final_strat.sort_values("yield_rate", ascending=False),
        x="yield_rate",
        y="name",
        ax=axes[0, 1],
        palette="mako",
    )
    axes[0, 1].set_title("Coverage Yield Rate (Cov Increase / Attempts)")
    axes[0, 1].set_xlabel("Yield Rate")

    # Plot C: Syntax Error Rate (Are they breaking the AST too much?)
    sns.barplot(
        data=final_strat.sort_values("syntax_err_rate", ascending=False),
        x="syntax_err_rate",
        y="name",
        ax=axes[1, 0],
        palette="rocket",
    )
    axes[1, 0].set_title("Syntax Error Rate (Syntax Err / Attempts)")
    axes[1, 0].set_xlabel("Error Rate")

    # Plot D: Total Attempts (Did the scheduler actually use them?)
    sns.barplot(
        data=final_strat.sort_values("attempts", ascending=False),
        x="attempts",
        y="name",
        ax=axes[1, 1],
        palette="cubehelix",
    )
    axes[1, 1].set_title("Total Attempts Allocated")
    axes[1, 1].set_xlabel("Total Attempts")

    plt.tight_layout()
    plt.savefig("strategy_efficiency.png", dpi=300, bbox_inches="tight")
    plt.close()

    # --- 3. Corpus Scheduler Behavior Dashboard ---
    fig, axes = plt.subplots(1, 3, figsize=(20, 6))
    fig.suptitle("Corpus Scheduler Health & Weighting Bias", fontsize=18)

    # Plot A: Is there a recency bias? (Rating vs Introduction Time)
    sns.scatterplot(data=final_sched, x="intro_tick", y="rating", alpha=0.6, ax=axes[0])
    axes[0].set_title("Final Rating vs. Entry Introduction Tick")
    axes[0].set_xlabel("Introduction Tick (Older -> Newer)")
    axes[0].set_ylabel("Final Rating")

    # Plot B: Are old entries starved? (Attempts vs Introduction Time)
    sns.scatterplot(
        data=final_sched, x="intro_tick", y="attempts", alpha=0.6, ax=axes[1], color="coral"
    )
    axes[1].set_title("Total Attempts vs. Entry Introduction Tick")
    axes[1].set_xlabel("Introduction Tick (Older -> Newer)")
    axes[1].set_ylabel("Total Attempts")

    # Plot C: What actually drives the rating? (Correlation Matrix)
    # This helps check if your weight formula is doing what you think it is
    corr_cols = ["attempts", "accepted", "cov_increase", "rating", "probability"]
    corr_matrix = final_sched[corr_cols].corr()
    sns.heatmap(corr_matrix, annot=True, cmap="coolwarm", vmin=-1, vmax=1, ax=axes[2], fmt=".2f")
    axes[2].set_title("Corpus Metric Correlations")

    plt.tight_layout()
    plt.savefig("corpus_scheduler_behavior.png", dpi=300, bbox_inches="tight")
    plt.close()


# Run the script
# advanced_fuzzer_analysis('strategy_stats.csv', 'scheduler_stats.csv')

if __name__ == "__main__":
    advanced_fuzzer_analysis(
        "../../docker_out/perf_out/strategy_stats.csv",
        "../../docker_out/perf_out/scheduler_stats.csv",
    )
