---
name: agents-hive-workflow
description: GitHub CLI commands specifically for agents hive workflow with project board management
license: MIT
compatibility: opencode
metadata:
  tool: github-cli
  category: workflow-automation
  audience: ai-agents
---

# Agents Hive Workflow - GitHub CLI Commands

## Purpose
Specialized commands for AI agents to manage GitHub Issues and Project Board in the agents hive workflow.

## Project Configuration
- **Project Number**: 2
- **Owner**: digit1024
- **Repository**: LunaAI
- **Columns**: Backlog, Ready, In Progress, In Review, QA, Done

## Core Commands

### 1. Move Issue Between Columns
```bash
# Move issue to specific column
move_issue_to_column() {
  local issue_number=$1
  local column_name=$2
  
  # Get item ID
  local item_id=$(gh project item-list 2 --owner digit1024 --format json | \
    jq -r ".items[] | select(.content.number == $issue_number) | .id")
  
  # Get Status field ID
  local field_id=$(gh project field-list 2 --owner digit1024 --format json | \
    jq -r '.fields[] | select(.name == "Status") | .id')
  
  # Get column option ID
  local column_id=$(gh project field-list 2 --owner digit1024 --format json | \
    jq -r ".fields[] | select(.name == \"Status\") | .options[] | select(.name == \"$column_name\") | .id")
  
  # Execute move
  gh project item-edit 2 --owner digit1024 --item-id "$item_id" \
    --field-id "$field_id" --project-id 2 \
    --single-select-option-id "$column_id"
  
  echo "Moved issue #$issue_number to '$column_name'"
}

# Usage examples:
# move_issue_to_column 123 "Ready"
# move_issue_to_column 123 "In Progress"
# move_issue_to_column 123 "In Review"
# move_issue_to_column 123 "QA"
# move_issue_to_column 123 "Done"
```

### 2. Add Role-Prefixed Comment
```bash
# Add comment with agent role prefix
add_agent_comment() {
  local issue_number=$1
  local role=$2  # ARCHITECT, CODER, REVIEWER, QA
  local message=$3
  
  gh issue comment "$issue_number" --body "$role: $message"
}

# Usage:
# add_agent_comment 123 "ARCHITECT" "Analysis complete. Architecture designed."
# add_agent_comment 123 "CODER" "Implementation complete. PR #45 created."
# add_agent_comment 123 "REVIEWER" "Code review completed. Approved."
# add_agent_comment 123 "QA" "All tests pass. Ready for deployment."
```

### 3. Create PR Linked to Issue
```bash
# Create PR and link to issue
create_pr_for_issue() {
  local issue_number=$1
  local title=$2
  local body=$3
  local branch=$4
  
  # Create PR
  local pr_url=$(gh pr create \
    --title "$title" \
    --body "$body\n\nCloses #$issue_number" \
    --base main \
    --head "$branch" \
    --json url --jq '.url')
  
  echo "$pr_url"
}

# Usage:
# create_pr_for_issue 123 "Feature: Add authentication" "Implements JWT auth" "feature/auth"
```

## Agent-Specific Workflows

### Architect Agent
```bash
# Complete architect workflow
architect_workflow() {
  local issue_number=$1
  local analysis=$2
  
  # 1. Add analysis comment
  add_agent_comment "$issue_number" "ARCHITECT" "$analysis"
  
  # 2. Move to Ready column
  move_issue_to_column "$issue_number" "Ready"
  
  echo "ARCHITECT: Issue #$issue_number analyzed and moved to Ready"
}
```

### Coder Agent
```bash
# Complete coder workflow
coder_workflow() {
  local issue_number=$1
  local implementation_summary=$2
  local pr_title=$3
  local pr_body=$4
  local branch_name=$5
  
  # 1. Move to In Progress
  move_issue_to_column "$issue_number" "In Progress"
  
  # 2. Implement code (external process)
  
  # 3. Create PR
  local pr_url=$(create_pr_for_issue "$issue_number" "$pr_title" "$pr_body" "$branch_name")
  
  # 4. Add comment with PR link
  add_agent_comment "$issue_number" "CODER" "Implementation complete. $implementation_summary\nPR: $pr_url"
  
  # 5. Move to In Review
  move_issue_to_column "$issue_number" "In Review"
  
  echo "CODER: Issue #$issue_number implemented. PR created: $pr_url"
}
```

### Reviewer Agent
```bash
# Complete reviewer workflow
reviewer_workflow() {
  local issue_number=$1
  local pr_number=$2
  local review_result=$3  # "approve" or "request-changes"
  local review_comment=$4
  
  # 1. Add review to PR
  gh pr review "$pr_number" "--$review_result" --body "REVIEWER: $review_comment"
  
  # 2. Add issue comment
  add_agent_comment "$issue_number" "REVIEWER" "Code review completed. $review_comment"
  
  # 3. Move based on review result
  if [ "$review_result" = "approve" ]; then
    move_issue_to_column "$issue_number" "QA"
    echo "REVIEWER: Issue #$issue_number approved and moved to QA"
  else
    move_issue_to_column "$issue_number" "Ready"
    echo "REVIEWER: Issue #$issue_number needs changes, moved back to Ready"
  fi
}
```

### QA Agent
```bash
# Complete QA workflow
qa_workflow() {
  local issue_number=$1
  local test_results=$2
  local decision=$3  # "approve" or "reject"
  
  # 1. Add test results comment
  add_agent_comment "$issue_number" "QA" "Test results: $test_results"
  
  # 2. Move based on decision
  if [ "$decision" = "approve" ]; then
    move_issue_to_column "$issue_number" "Done"
    echo "QA: Issue #$issue_number approved and moved to Done"
  else
    move_issue_to_column "$issue_number" "Ready"
    echo "QA: Issue #$issue_number rejected, moved back to Ready"
  fi
}
```

## Utility Functions

### Get Current Column
```bash
# Check which column an issue is in
get_issue_column() {
  local issue_number=$1
  
  gh project item-list 2 --owner digit1024 --format json | \
    jq -r ".items[] | select(.content.number == $issue_number) | .fieldValues.nodes[] | select(.field.name == \"Status\") | .name"
}

# Usage:
# column=$(get_issue_column 123)
# echo "Issue #123 is in: $column"
```

### List Issues in Column
```bash
# List all issues in a specific column
list_issues_in_column() {
  local column_name=$1
  
  gh project item-list 2 --owner digit1024 --format json | \
    jq -r ".items[] | select(.fieldValues.nodes[].name == \"$column_name\") | .content.number"
}

# Usage:
# list_issues_in_column "Backlog"
# list_issues_in_column "Ready"
# list_issues_in_column "In Progress"
```

### Validate Issue in Correct Column
```bash
# Check if issue is in expected column
validate_issue_column() {
  local issue_number=$1
  local expected_column=$2
  
  local current_column=$(get_issue_column "$issue_number")
  
  if [ "$current_column" = "$expected_column" ]; then
    echo "✓ Issue #$issue_number is in $expected_column"
    return 0
  else
    echo "✗ Issue #$issue_number is in $current_column (expected: $expected_column)"
    return 1
  fi
}
```

## Complete Workflow Example

```bash
#!/bin/bash
# complete_workflow_example.sh
# Demonstrates full agents hive workflow for issue #123

ISSUE_NUMBER=123

echo "=== Starting Agents Hive Workflow for Issue #$ISSUE_NUMBER ==="

# 1. ARCHITECT phase
echo "1. ARCHITECT analyzing issue..."
architect_workflow "$ISSUE_NUMBER" "Analysis complete. Architecture designed for user authentication feature."

# 2. CODER phase
echo "2. CODER implementing issue..."
coder_workflow "$ISSUE_NUMBER" \
  "Implemented JWT authentication with tests" \
  "Feature: Add user authentication" \
  "Implements JWT-based authentication system with comprehensive tests" \
  "feature/auth"

# 3. REVIEWER phase
echo "3. REVIEWER reviewing PR..."
reviewer_workflow "$ISSUE_NUMBER" \
  45 \
  "approve" \
  "Code looks good. Minor suggestions provided. Approved for QA."

# 4. QA phase
echo "4. QA testing implementation..."
qa_workflow "$ISSUE_NUMBER" \
  "All unit and integration tests pass. Security scan clean. Performance within limits." \
  "approve"

echo "=== Workflow complete for Issue #$ISSUE_NUMBER ==="
```

## Error Handling

```bash
# Safe move function with validation
safe_move_issue() {
  local issue_number=$1
  local target_column=$2
  
  # Check if issue exists
  if ! gh issue view "$issue_number" >/dev/null 2>&1; then
    echo "Error: Issue #$issue_number not found"
    return 1
  fi
  
  # Check if column exists
  local column_exists=$(gh project field-list 2 --owner digit1024 --format json | \
    jq -r ".fields[] | select(.name == \"Status\") | .options[] | select(.name == \"$target_column\") | .name")
  
  if [ -z "$column_exists" ]; then
    echo "Error: Column '$target_column' not found in project"
    return 1
  fi
  
  # Execute move
  move_issue_to_column "$issue_number" "$target_column"
}

# Check prerequisites
check_prerequisites() {
  # Check gh authentication
  if ! gh auth status >/dev/null 2>&1; then
    echo "Error: GitHub CLI not authenticated"
    echo "Run: gh auth login"
    return 1
  fi
  
  # Check jq installation
  if ! command -v jq >/dev/null 2>&1; then
    echo "Error: jq is required but not installed"
    return 1
  fi
  
  # Check project access
  if ! gh project view 2 --owner digit1024 >/dev/null 2>&1; then
    echo "Error: Cannot access project 2"
    return 1
  fi
  
  echo "✓ All prerequisites met"
  return 0
}
```

## Quick Reference

### For ARCHITECT:
```bash
move_issue_to_column <issue> "Ready"
add_agent_comment <issue> "ARCHITECT" "<analysis>"
```

### For CODER:
```bash
move_issue_to_column <issue> "In Progress"
# ... implement ...
create_pr_for_issue <issue> "<title>" "<body>" "<branch>"
move_issue_to_column <issue> "In Review"
```

### For REVIEWER:
```bash
gh pr review <pr> --approve --body "REVIEWER: <comment>"
move_issue_to_column <issue> "QA"
```

### For QA:
```bash
add_agent_comment <issue> "QA" "<test results>"
move_issue_to_column <issue> "Done"
```

## Notes
1. Always run `check_prerequisites` before starting workflow
2. Use `safe_move_issue` for production use
3. Log all actions for audit trail
4. Handle errors gracefully in scripts